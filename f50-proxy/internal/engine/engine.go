// Package engine owns the running Xray instance: it starts and stops runs,
// tracks their traffic, runs the 策略组 and watches the network and the node.
package engine

import (
	"context"
	"errors"
	"fmt"
	"math"
	"net"
	"os"
	"slices"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"f50/proxy/internal/link"
	"f50/proxy/internal/logtap"
	"f50/proxy/internal/netmark"
	"f50/proxy/internal/platform"
	"f50/proxy/internal/route"
	"f50/proxy/internal/router"
	"f50/proxy/internal/tracker"
	"f50/proxy/internal/version"
	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/features/stats"
)

type StartRequest struct {
	Link    string `json:"link"`
	Name    string `json:"name"`
	Mode    string `json:"mode"` // proxy | gateway
	Routing string `json:"routing"`
	// RulesJSON is the route document (see CompileRoute).
	RulesJSON string `json:"rules_json"`
	// RuleSets is the One Node rule catalogue (…/api/v1/one/rules/) rule
	// sets load from when neither inline nor built in.
	RuleSets string `json:"rule_sets"`
	Port     int    `json:"port"` // mixed inbound, default 17890
	// TProxyPort is the gateway's transparent inbound (default 17892), Mark
	// the SO_MARK on every socket the core opens (default 255).
	TProxyPort int `json:"tproxy_port"`
	Mark       int `json:"mark"`
	// Nodes are the nodes rules and groups may name; Selections the saved
	// node of each select group.
	Nodes      []NamedLink       `json:"nodes"`
	Selections map[string]string `json:"selections"`
	// Via are the picked node's front nodes (前置代理), nearest first.
	Via []string `json:"via"`
	// AppHosts are hosts that go direct while the node is down
	// (Policy.AppHosts).
	AppHosts []string `json:"app_hosts"`
}

type NamedLink struct {
	Name string   `json:"name"`
	Link string   `json:"link"`
	Via  []string `json:"via"`
}

type Status struct {
	Running    bool      `json:"running"`
	Mode       string    `json:"mode,omitempty"`
	Routing    string    `json:"routing,omitempty"`
	Node       string    `json:"node,omitempty"`
	Kind       string    `json:"kind,omitempty"`
	Port       int       `json:"port,omitempty"`
	TProxyPort int       `json:"tproxy_port,omitempty"`
	Mark       int       `json:"mark,omitempty"`
	Since      time.Time `json:"since"`
	Up         int64     `json:"up"`
	Down       int64     `json:"down"`
	Elevated   bool      `json:"elevated"`
	Xray       string    `json:"xray"`
	// Version is this engine's own release (version.Version).
	Version string `json:"version"`
	// Mem is this process's memory in bytes (see platform.MemBytes).
	Mem uint64 `json:"mem"`
	// Groups while running (the route's 策略组, with delays and picks).
	Groups []GroupStatus `json:"groups,omitempty"`
	// Health while running: the network and the node as last checked (health.go).
	Health *Health `json:"health,omitempty"`
}

// Engine owns at most one running Xray instance.
type Engine struct {
	logLevel string

	tracker *tracker.Tracker

	mu     sync.Mutex
	inst   *core.Instance
	up     []uplink        // the node outbounds'
	down   []stats.Counter // the node outbounds'
	groups *groupSet
	health *healthWatcher
	status Status
}

func New(logLevel string) *Engine {
	return &Engine{logLevel: logLevel, tracker: tracker.New()}
}

// Start runs the request, replacing a running instance. Why a start failed is
// always in the log, whichever step refused.
func (e *Engine) Start(req StartRequest) (Status, error) {
	began := time.Now()
	st, err := e.start(req)
	if err != nil {
		logtap.Warn(fmt.Sprintf("start %q failed after %s (mode %s, routing %s, port %d): %v",
			req.Name, time.Since(began).Round(time.Millisecond), req.Mode, req.Routing, req.Port, err))
	}
	return st, err
}

// normalize checks the request and fills in its defaults.
func (req *StartRequest) normalize() error {
	if req.Mode != xrayconf.ModeProxy && req.Mode != xrayconf.ModeGateway {
		return fmt.Errorf("unknown mode %q", req.Mode)
	}
	switch req.Routing {
	case "":
		req.Routing = xrayconf.RoutingGlobal
	case xrayconf.RoutingGlobal, xrayconf.RoutingRule, xrayconf.RoutingDirect:
	default:
		return fmt.Errorf("unknown routing %q", req.Routing)
	}
	if req.Port == 0 {
		req.Port = xrayconf.DefaultPort
	}
	if req.Port < 0 || req.Port > 65535 {
		return fmt.Errorf("bad port %d", req.Port)
	}
	if req.Mode != xrayconf.ModeGateway {
		req.TProxyPort, req.Mark = 0, 0
		return nil
	}
	if req.TProxyPort == 0 {
		req.TProxyPort = xrayconf.DefaultTProxyPort
	}
	if req.TProxyPort < 0 || req.TProxyPort > 65535 || req.TProxyPort == req.Port {
		return fmt.Errorf("bad tproxy port %d", req.TProxyPort)
	}
	if req.Mark == 0 {
		req.Mark = xrayconf.DefaultMark
	}
	if req.Mark < 0 || req.Mark > math.MaxInt32 {
		return fmt.Errorf("bad mark %d", req.Mark)
	}
	return nil
}

// runPlan is a start request compiled: the Xray document and what the
// router and the counters need besides it.
type runPlan struct {
	doc    xrayconf.Obj
	route  route.Route
	policy *route.Policy
	groups *groupSet
	kind   string
	tags   []string // tracked outbounds: proxy, direct, block, then the other nodes
	others int      // nodes the request named
}

// plan compiles a normalized request into a run, without starting anything.
func plan(req StartRequest, logLevel string) (*runPlan, error) {
	// Nodes other than the picked one get an outbound when the route uses them.
	links, names := map[string][]xrayconf.Obj{}, []string{}
	for _, n := range req.Nodes {
		if _, dup := links[n.Name]; dup || n.Name == "" {
			continue
		}
		if _, obs, err := link.ChainOutbounds(n.Link, n.Via, route.NodeTag(n.Name)); err == nil {
			links[n.Name] = obs
			names = append(names, n.Name)
		}
	}
	rt, err := route.CompileRoute(req.RulesJSON, route.CatalogueAt(req.RuleSets), names...)
	if err != nil {
		return nil, err
	}
	policy, err := route.NewPolicy(req.Routing, rt)
	if err != nil {
		return nil, err
	}
	groups := newGroupSet(rt.Groups, req.Selections)
	policy.Resolve = groups.resolve
	for _, h := range req.AppHosts {
		if h = strings.Trim(strings.ToLower(h), ". "); h != "" {
			policy.AppHosts = append(policy.AppHosts, h)
		}
	}
	kind, proxy, err := link.ChainOutbounds(req.Link, req.Via, "proxy")
	if err != nil {
		return nil, err
	}
	listen := xrayconf.Listen{Mode: req.Mode, Port: req.Port, TProxyPort: req.TProxyPort, Mark: req.Mark}
	doc := xrayconf.Document(proxy[0], listen, req.Routing, logLevel)
	xrayconf.AppHostsDNS(doc, req.Routing, policy.AppHosts)
	// The nodes' dialers (fragment, 前置代理) stay untracked: their traffic is the node's.
	outbounds := append(doc["outbounds"].([]xrayconf.Obj), proxy[1:]...)
	tags := []string{"proxy", "direct", "block"}
	for _, name := range rt.Nodes {
		outbounds = append(outbounds, links[name]...)
		tags = append(tags, route.NodeTag(name))
	}
	xrayconf.MarkSockets(outbounds, req.Mark)
	doc["outbounds"] = outbounds
	return &runPlan{doc: doc, route: rt, policy: policy, groups: groups, kind: kind, tags: tags, others: len(names)}, nil
}

func (e *Engine) start(req StartRequest) (Status, error) {
	began := time.Now()
	if err := req.normalize(); err != nil {
		return Status{}, err
	}
	// The core's own sockets (rule sets, resolving the node) pass the
	// nftables rules from here on.
	netmark.Set(req.Mark)
	p, err := plan(req, e.logLevel)
	if err != nil {
		return Status{}, err
	}
	config, err := xrayconf.Build(p.doc)
	if err != nil {
		return Status{}, fmt.Errorf("build config: %w", err)
	}
	if req.Mode == xrayconf.ModeGateway {
		// Dialing the node by name would ask the core's own DNS, which
		// reaches the node through itself; resolve it before the start.
		if err := resolveServers(config); err != nil {
			return Status{}, err
		}
	}
	rt, policy, groups, tags := p.route, p.policy, p.groups, p.tags

	e.mu.Lock()
	defer e.mu.Unlock()
	e.stopLocked()
	inst, err := core.NewWithContext(router.WithPolicy(context.Background(), config, policy), config)
	if err != nil {
		return Status{}, fmt.Errorf("init xray: %w", err)
	}
	logtap.Install(inst)
	if err := e.tracker.Wrap(inst, tags...); err != nil {
		_ = inst.Close()
		return Status{}, fmt.Errorf("track connections: %w", err)
	}
	if err := inst.Start(); err != nil {
		_ = inst.Close()
		logtap.Install(nil)
		return Status{}, fmt.Errorf("start xray: %w", err)
	}
	e.inst = inst
	if m, ok := inst.GetFeature(stats.ManagerType()).(stats.Manager); ok {
		for _, tag := range slices.Concat([]string{"proxy"}, tags[3:]) {
			e.up = append(e.up, uplink{m.GetCounter("outbound>>>" + tag + ">>>traffic>>>uplink"), e.tracker.Sent(tag)})
			if c := m.GetCounter("outbound>>>" + tag + ">>>traffic>>>downlink"); c != nil {
				e.down = append(e.down, c)
			}
		}
	}
	groups.test = instanceProbe(inst)
	groups.run()
	e.groups = groups
	e.status = Status{Running: true, Mode: req.Mode, Routing: req.Routing, Node: req.Name, Kind: p.kind,
		Port: req.Port, TProxyPort: req.TProxyPort, Mark: req.Mark, Since: time.Now()}
	logtap.Note(fmt.Sprintf("started %q (%s, mode %s, routing %s, port %d)", req.Name, p.kind, req.Mode, req.Routing, req.Port))
	// What this run is made of, for reading the log later. No credentials.
	server := "?"
	if p, err := link.Parse(req.Link); err == nil {
		server = net.JoinHostPort(p.Server, strconv.Itoa(p.Port))
	}
	logtap.Note(fmt.Sprintf("run: server %s, %d front node(s), %d rules (final %s), %d group(s), %d of %d other nodes in use, dns %s, loglevel %s, pid %d, up in %s",
		server, len(req.Via), rt.Count, rt.Final, len(rt.Groups), len(rt.Nodes), p.others, xrayconf.DNSSummary(req.Routing), e.logLevel, os.Getpid(), time.Since(began).Round(time.Millisecond)))
	if req.Mode == xrayconf.ModeGateway {
		logtap.Note(fmt.Sprintf("gateway: tproxy on [::]:%d, socket mark %#x, elevated %v", req.TProxyPort, req.Mark, platform.IsElevated()))
	}
	for _, w := range rt.Warnings {
		logtap.Warn("run: rules warning: " + w)
	}
	src := healthSources{NodeDown: &policy.NodeDown, traffic: func() (int64, int64) {
		st := e.Status()
		return st.Up, st.Down
	}}
	if req.Routing != xrayconf.RoutingDirect {
		src.checkNode = proxyProbe(inst)
	}
	e.health = watchHealth(req.Name, e.tracker, src)
	return e.statusLocked(), nil
}

// Stop closes Xray and returns the final status of the run it stopped.
func (e *Engine) Stop() Status {
	e.mu.Lock()
	defer e.mu.Unlock()
	last := e.statusLocked()
	e.stopLocked()
	netmark.Set(0)
	last.Running = false
	return last
}

func (e *Engine) stopLocked() {
	if e.inst != nil {
		st := e.statusLocked()
		// Closing the instance stops its listeners but not the connections it
		// carries; end them, so apps reconnect through the next instance's node.
		closed := e.tracker.Close(0)
		began := time.Now()
		err := e.inst.Close()
		logtap.Install(nil)
		logtap.Note(fmt.Sprintf("stopped %q after %s (closing took %s), up %d B, down %d B, closed %d connections",
			st.Node, time.Since(st.Since).Round(time.Second), time.Since(began).Round(time.Millisecond), st.Up, st.Down, closed))
		if err != nil {
			logtap.Warn(fmt.Sprintf("stop: closing xray: %v", err))
		}
	}
	if e.groups != nil {
		e.groups.close()
	}
	if e.health != nil {
		e.health.stop()
	}
	e.inst, e.up, e.down, e.groups, e.health = nil, nil, nil, nil, nil
	e.status = Status{}
}

// uplink is a node outbound's upload: Xray's stats counter, or the bytes the
// tracker saw leave through it when that is more. Trojan, VMess and
// Shadowsocks write through BufferedWriter.Write once the header is out, which
// goes straight to the connection under Xray's counter, so their counter
// stays near zero.
type uplink struct {
	counter stats.Counter
	sent    *atomic.Int64
}

func (u uplink) value() int64 {
	var n int64
	if u.counter != nil {
		n = u.counter.Value()
	}
	if u.sent != nil {
		n = max(n, u.sent.Load())
	}
	return n
}

// Tracker is the engine's connection tracker (the 连接 page).
func (e *Engine) Tracker() *tracker.Tracker { return e.tracker }

func (e *Engine) Status() Status {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.statusLocked()
}

func (e *Engine) statusLocked() Status {
	s := e.status
	for _, u := range e.up {
		s.Up += u.value()
	}
	for _, c := range e.down {
		s.Down += c.Value()
	}
	if e.groups != nil {
		s.Groups = e.groups.Status()
	}
	if e.health != nil {
		s.Health = e.health.snapshot()
	}
	s.Elevated = platform.IsElevated()
	s.Xray = core.Version()
	s.Version = version.Version
	s.Mem = platform.MemBytes()
	return s
}

// Groups returns the running route's groups.
func (e *Engine) Groups() ([]GroupStatus, error) {
	g, err := e.groupSet()
	if err != nil {
		return nil, err
	}
	return g.Status(), nil
}

// SelectGroup points a select group at a node. New connections take it, and
// the group's open connections through its old node are closed, so the apps
// reconnect through the new one (keep-alive and HTTP/2 connections would
// otherwise keep the old exit). Other groups' and rules' connections stay.
func (e *Engine) SelectGroup(group, node string) ([]GroupStatus, error) {
	g, err := e.groupSet()
	if err != nil {
		return nil, err
	}
	before, err := g.Select(group, node)
	if err != nil {
		return nil, err
	}
	closed := 0
	if before != node {
		closed = e.tracker.CloseWhere(func(c *tracker.Conn) bool { return c.Group == group && c.Outbound != route.NodeTag(node) })
	}
	logtap.Note(fmt.Sprintf("group %q: picked %s, closed %d connections", group, node, closed))
	return g.Status(), nil
}

// TestGroup times a group's nodes now (the lock is not held meanwhile).
func (e *Engine) TestGroup(ctx context.Context, group string) ([]GroupStatus, error) {
	g, err := e.groupSet()
	if err != nil {
		return nil, err
	}
	if err := g.Test(ctx, group); err != nil {
		return nil, err
	}
	return g.Status(), nil
}

func (e *Engine) groupSet() (*groupSet, error) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.groups == nil {
		return nil, errors.New("未连接")
	}
	return e.groups, nil
}
