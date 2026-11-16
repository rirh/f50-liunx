package engine

import (
	"context"
	"fmt"
	"net"
	"slices"
	"sync"
	"time"

	"f50/proxy/internal/logtap"
	"f50/proxy/internal/probe"
	"f50/proxy/internal/route"
	"f50/proxy/internal/xrayconf"

	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/common/session"
	"github.com/xtls/xray-core/core"
)

// 策略组 at run time. Every node a route uses has its own Xray outbound
// (route.NodeTag); the router asks the group set which node a group's tag means
// right now. urltest and fallback groups test their members through the
// running instance, the way their traffic goes.

const (
	groupProbeTimeout = 5 * time.Second
	// urltest moves off its node only for one this much faster (sing-box's default).
	urlTestTolerance = 50
)

// GroupStatus is one group as f50d shows it.
type GroupStatus struct {
	Name     string       `json:"name"`
	Type     string       `json:"type"`
	Selected string       `json:"selected"`
	Nodes    []NodeHealth `json:"nodes"`
}

// NodeHealth is a member's last test: no delay until tested, alive until a test fails.
type NodeHealth struct {
	Name    string `json:"name"`
	DelayMs *int64 `json:"delay_ms"`
	Alive   bool   `json:"alive"`
}

// delay is one node's last test; ms 0 means it failed.
type delay struct{ ms int64 }

type groupSet struct {
	groups []*groupState
	// test times one request through a node; set once the instance exists.
	test func(ctx context.Context, node string) (int64, error)

	mu     sync.Mutex
	delays map[string]delay // by node; absent: not tested
	stop   context.CancelFunc
}

type groupState struct {
	route.Group
	selected string
}

// newGroupSet starts each select group on the saved choice (or its first
// node) and the others on their first node until tested.
func newGroupSet(groups []route.Group, selections map[string]string) *groupSet {
	s := &groupSet{delays: map[string]delay{}}
	for _, g := range groups {
		state := &groupState{Group: g, selected: g.Nodes[0]}
		if pick := selections[g.Name]; g.Type == route.GroupSelect && slices.Contains(g.Nodes, pick) {
			state.selected = pick
		}
		s.groups = append(s.groups, state)
	}
	return s
}

func (s *groupSet) find(name string) *groupState {
	for _, g := range s.groups {
		if g.Name == name {
			return g
		}
	}
	return nil
}

// resolve is the policy's Resolve: a group's tag becomes its node's.
func (s *groupSet) resolve(tag string) string {
	name, ok := route.CutGroupTag(tag)
	if !ok {
		return tag
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if g := s.find(name); g != nil {
		return route.NodeTag(g.selected)
	}
	return "proxy"
}

// Select sets a select group's node and returns the one it replaced.
func (s *groupSet) Select(group, node string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	g := s.find(group)
	switch {
	case g == nil:
		return "", fmt.Errorf("策略组 %s 不存在", group)
	case g.Type != route.GroupSelect:
		return "", fmt.Errorf("策略组 %s 自动选择节点，不能手动选", group)
	case !slices.Contains(g.Nodes, node):
		return "", fmt.Errorf("节点 %s 不在策略组 %s 里", node, group)
	}
	before := g.selected
	g.selected = node
	return before, nil
}

// Test times every member of one group now, then lets the group re-pick.
func (s *groupSet) Test(ctx context.Context, group string) error {
	s.mu.Lock()
	g := s.find(group)
	s.mu.Unlock()
	if g == nil {
		return fmt.Errorf("策略组 %s 不存在", group)
	}
	s.testGroup(ctx, g)
	return nil
}

func (s *groupSet) testGroup(ctx context.Context, g *groupState) {
	results := make([]delay, len(g.Nodes))
	var wg sync.WaitGroup
	for i, node := range g.Nodes {
		wg.Go(func() {
			ctx, cancel := context.WithTimeout(ctx, groupProbeTimeout)
			defer cancel()
			if ms, err := s.test(ctx, node); err == nil {
				results[i] = delay{ms}
			}
		})
	}
	wg.Wait()
	if ctx.Err() != nil {
		return // stopped mid-test: the results say nothing about the nodes
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for i, node := range g.Nodes {
		s.delays[node] = results[i]
	}
	before := g.selected
	switch g.Type {
	case route.GroupURLTest:
		g.selected = pickURLTest(g.Nodes, s.delays, g.selected)
	case route.GroupFallback:
		g.selected = pickFallback(g.Nodes, s.delays)
	}
	if g.selected != before {
		logtap.Note(fmt.Sprintf("group %q: %s → %s", g.Name, before, g.selected))
	}
}

// pickURLTest takes the fastest node that answered, staying on the current
// one unless another is faster by the tolerance or it failed. With no answer
// at all the current node stays.
func pickURLTest(nodes []string, delays map[string]delay, current string) string {
	best := ""
	for _, node := range nodes {
		if d := delays[node]; d.ms > 0 && (best == "" || d.ms < delays[best].ms) {
			best = node
		}
	}
	if best == "" {
		return current
	}
	if d := delays[current]; d.ms > 0 && slices.Contains(nodes, current) && d.ms <= delays[best].ms+urlTestTolerance {
		return current
	}
	return best
}

// pickFallback takes the first node that has not failed (untested counts as
// up); if all failed, the first.
func pickFallback(nodes []string, delays map[string]delay) string {
	for _, node := range nodes {
		if d, tested := delays[node]; !tested || d.ms > 0 {
			return node
		}
	}
	return nodes[0]
}

// run tests urltest and fallback groups at once and then every interval, until stopped.
func (s *groupSet) run() {
	ctx, cancel := context.WithCancel(context.Background())
	s.stop = cancel
	for _, g := range s.groups {
		if g.Type == route.GroupSelect {
			continue
		}
		go func() {
			ticker := time.NewTicker(time.Duration(g.Interval) * time.Second)
			defer ticker.Stop()
			for {
				s.testGroup(ctx, g)
				select {
				case <-ctx.Done():
					return
				case <-ticker.C:
				}
			}
		}()
	}
}

func (s *groupSet) close() {
	if s.stop != nil {
		s.stop()
	}
}

// Status lists the groups in the document's order.
func (s *groupSet) Status() []GroupStatus {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]GroupStatus, 0, len(s.groups))
	for _, g := range s.groups {
		st := GroupStatus{Name: g.Name, Type: g.Type, Selected: g.selected, Nodes: make([]NodeHealth, 0, len(g.Nodes))}
		for _, node := range g.Nodes {
			h := NodeHealth{Name: node, Alive: true}
			if d, tested := s.delays[node]; tested {
				h.Alive = d.ms > 0
				if h.Alive {
					ms := d.ms
					h.DelayMs = &ms
				}
			}
			st.Nodes = append(st.Nodes, h)
		}
		out = append(out, st)
	}
	return out
}

// instanceProbe times a request through one node's outbound of a running instance.
func instanceProbe(inst *core.Instance) func(context.Context, string) (int64, error) {
	return func(ctx context.Context, node string) (int64, error) {
		// CanSpliceCopy 3: never splice, as for a dial without an inbound.
		ctx = session.ContextWithInbound(ctx, &session.Inbound{Tag: xrayconf.ProbeInbound + route.NodeTag(node), CanSpliceCopy: 3})
		return probe.HTTPDelay(ctx, func(ctx context.Context, dest xnet.Destination) (net.Conn, error) {
			return core.Dial(ctx, inst, dest)
		}, "")
	}
}
