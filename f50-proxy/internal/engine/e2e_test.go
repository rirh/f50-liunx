package engine

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/xtls/xray-core/features/stats"

	"f50/proxy/internal/link"
	"f50/proxy/internal/logtap"
	"f50/proxy/internal/probe"
	"f50/proxy/internal/route"
	"f50/proxy/internal/testserver"
	"f50/proxy/internal/tracker"
	"f50/proxy/internal/xrayconf"
)

// TestLoopbackLines drives the client engine through a real Xray VLESS REALITY
// server (testserver.Run, in a child process) exactly as the app does: share
// link -> Start -> HTTP and SOCKS requests on the mixed port -> traffic counters,
// plus the delay probe.
func TestLoopbackLines(t *testing.T) {
	target, links := startTestServer(t)
	u, _ := url.Parse(target)
	probe.Host = u.Hostname()
	probe.Port, _ = strconv.Atoi(u.Port())

	nodes := link.ParseSubscription(base64.StdEncoding.EncodeToString([]byte(strings.Join(links, "\n"))))
	if len(nodes) != 2 {
		t.Fatalf("parse: %+v", nodes)
	}
	for _, node := range nodes {
		t.Run(node.Name, func(t *testing.T) {
			if node.Problem != "" || node.Kind == "" {
				t.Fatalf("node %+v", node)
			}
			engine := New("warning")
			mixed := testserver.FreeTCPPort()
			// 规则 mode: e2e.test is not mainland China, so it must still go through the node.
			if _, err := engine.Start(StartRequest{Link: node.Link, Name: node.Name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingRule, Port: mixed}); err != nil {
				t.Fatal(err)
			}
			defer engine.Stop()
			for _, scheme := range []string{"http", "socks5h"} {
				if body := get(t, scheme, mixed, target); body != "hello through the node" {
					t.Fatalf("%s proxy body %q", scheme, body)
				}
			}
			st := engine.Status()
			if !st.Running || st.Up == 0 || st.Down == 0 {
				t.Fatalf("status %+v", st)
			}
			// The 数据 page: both requests show up as counted connections through the node.
			conns := engine.tracker.Connections()
			seen := 0
			for _, c := range append(conns["active"].([]tracker.Conn), conns["closed"].([]tracker.Conn)...) {
				if c.Host == u.Hostname() && c.Outbound == "proxy" && c.Inbound == "mixed" && c.Source == "127.0.0.1" && c.Up > 0 && c.Down > 0 {
					seen++
				}
			}
			if seen < 2 {
				t.Fatalf("connections %+v", conns)
			}
			if closed := engine.tracker.Close(0); closed != len(conns["active"].([]tracker.Conn)) {
				t.Logf("closed %d of %d open connections", closed, len(conns["active"].([]tracker.Conn)))
			}
			lines := logtap.Since(logtap.Epoch, 0)["lines"].([]logtap.LogLine)
			if !slices.ContainsFunc(lines, func(l logtap.LogLine) bool { return strings.Contains(l.Text, "f50-proxy: started \""+node.Name) }) {
				t.Fatalf("log lines %+v", lines)
			}
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			ms, err := probe.Delay(ctx, node.Link)
			if err != nil {
				t.Fatalf("probe: %v", err)
			}
			t.Logf("%s (%s): up=%dB down=%dB probe=%dms", node.Name, node.Kind, st.Up, st.Down, ms)
		})
	}
}

// TestLoopbackProtocols carries real traffic through every other protocol the
// client dials (Shadowsocks, SOCKS5, HTTP, VMess, Trojan), plus the delay probe.
func TestLoopbackProtocols(t *testing.T) {
	target, _, others := startTestServerAll(t)
	nodes := link.ParseSubscription(strings.Join(others, "\n"))
	if len(nodes) != 6 {
		t.Fatalf("parse: %+v", nodes)
	}
	for _, node := range nodes {
		t.Run(node.Name, func(t *testing.T) {
			if node.Problem != "" {
				t.Fatalf("node %+v", node)
			}
			engine := New("warning")
			mixed := testserver.FreeTCPPort()
			if _, err := engine.Start(StartRequest{Link: node.Link, Name: node.Name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingGlobal, Port: mixed}); err != nil {
				t.Fatal(err)
			}
			defer engine.Stop()
			for _, scheme := range []string{"http", "socks5h"} {
				if body := get(t, scheme, mixed, target); body != "hello through the node" {
					t.Fatalf("%s proxy body %q", scheme, body)
				}
			}
			if st := engine.Status(); st.Up == 0 || st.Down == 0 {
				t.Fatalf("status %+v", st)
			}
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			u, _ := url.Parse(target)
			ms, err := probe.Node(ctx, node.Link, probe.Options{URL: "http://" + u.Host + "/generate_204"})
			if err != nil {
				t.Fatalf("probe: %v", err)
			}
			t.Logf("%s (%s, %s): probe=%dms", node.Name, node.Kind, node.Summary, ms)
		})
	}
}

// TestLoopbackGroups runs a document with 策略组 and 应用分流 through two real
// loopback nodes: each connection leaves by the outbound its rule names, a
// select group follows the pick, and urltest tests its nodes through the
// running instance.
func TestLoopbackGroups(t *testing.T) {
	target, links := startTestServer(t)
	u, _ := url.Parse(target)
	probe.Host = u.Hostname()
	probe.Port, _ = strconv.Atoi(u.Port())
	nodes := link.ParseSubscription(strings.Join(links, "\n"))
	named := []NamedLink{}
	for _, n := range nodes {
		named = append(named, NamedLink{Name: n.Name, Link: n.Link})
	}
	doc := `{
	  "groups": [
	    {"name": "手选", "type": "select", "nodes": ["reality-vision", "reality-xhttp"]},
	    {"name": "自动", "type": "urltest", "filter": "^reality-", "interval": 60}
	  ],
	  "rules": [{"domain": ["direct.e2e.test"], "outbound": "direct"}],
	  "apps": [{"app": "e2e", "outbound": "手选", "match": {"domain_suffix": ["e2e.test"]}}],
	  "final": "自动"
	}`
	engine := New("warning")
	mixed := testserver.FreeTCPPort()
	st, err := engine.Start(StartRequest{
		Link: nodes[0].Link, Name: nodes[0].Name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingRule, Port: mixed,
		RulesJSON: doc, Nodes: named, Selections: map[string]string{"手选": "reality-xhttp"},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Stop()
	if len(st.Groups) != 2 || st.Groups[0].Selected != "reality-xhttp" {
		t.Fatalf("groups %+v", st.Groups)
	}

	// lastOutbound is the outbound of the newest connection to host.
	lastOutbound := func(host string) string {
		conns := engine.tracker.Connections()
		var newest tracker.Conn
		for _, c := range append(conns["active"].([]tracker.Conn), conns["closed"].([]tracker.Conn)...) {
			if c.Host == host && c.ID > newest.ID {
				newest = c
			}
		}
		return newest.Outbound
	}
	if body := get(t, "http", mixed, target); body != "hello through the node" {
		t.Fatalf("body %q", body)
	}
	if got := lastOutbound(u.Hostname()); got != route.NodeTag("reality-xhttp") {
		t.Fatalf("e2e.test left by %q", got)
	}
	if _, err := engine.SelectGroup("手选", "reality-vision"); err != nil {
		t.Fatal(err)
	}
	if body := get(t, "socks5h", mixed, target); body != "hello through the node" {
		t.Fatalf("body %q", body)
	}
	if got := lastOutbound(u.Hostname()); got != route.NodeTag("reality-vision") {
		t.Fatalf("after the pick e2e.test left by %q", got)
	}
	// The direct rule: the name only exists behind the node, so the request fails, but it went direct.
	client := &http.Client{Timeout: 5 * time.Second, Transport: &http.Transport{Proxy: http.ProxyURL(&url.URL{Scheme: "http", Host: fmt.Sprintf("127.0.0.1:%d", mixed)})}}
	if resp, err := client.Get("http://direct.e2e.test:" + u.Port() + "/hello"); err == nil {
		resp.Body.Close()
	}
	if got := lastOutbound("direct.e2e.test"); got != "direct" {
		t.Fatalf("direct.e2e.test left by %q", got)
	}

	groups, err := engine.TestGroup(context.Background(), "自动")
	if err != nil {
		t.Fatal(err)
	}
	for _, n := range groups[1].Nodes {
		if n.DelayMs == nil || !n.Alive {
			t.Fatalf("自动 after a test: %+v", groups[1])
		}
	}
	t.Logf("自动: %s, delays %d / %d ms", groups[1].Selected, *groups[1].Nodes[0].DelayMs, *groups[1].Nodes[1].DelayMs)
	if _, err := engine.SelectGroup("自动", "reality-vision"); err == nil {
		t.Fatal("urltest accepted a pick")
	}
	// Tests are not the user's connections.
	conns := engine.tracker.Connections()
	for _, c := range append(conns["active"].([]tracker.Conn), conns["closed"].([]tracker.Conn)...) {
		if strings.HasPrefix(c.Inbound, xrayconf.ProbeInbound) {
			t.Fatalf("probe listed: %+v", c)
		}
	}
	if st := engine.Status(); st.Up == 0 || st.Down == 0 || len(st.Groups) != 2 {
		t.Fatalf("status %+v", st)
	}
	engine.Stop()
	if _, err := engine.Groups(); err == nil {
		t.Fatal("groups after stop")
	}
}

// TestRealNodesGroups is the same check on real nodes (F50PROXY_LINKS: a file
// of share links or a subscription; skipped without it). A rule sends
// www.cloudflare.com to a select group on the second node while the first is
// the picked one, another sends one.one.one.one direct; both report the
// egress address on /cdn-cgi/trace.
func TestRealNodesGroups(t *testing.T) {
	path := os.Getenv("F50PROXY_LINKS")
	if path == "" {
		t.Skip("set F50PROXY_LINKS to a file of share links")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var usable []NamedLink
	seen := map[string]bool{}
	for _, n := range link.ParseSubscription(string(raw)) {
		if n.Problem == "" && !seen[n.Name] {
			seen[n.Name] = true
			usable = append(usable, NamedLink{Name: n.Name, Link: n.Link})
		}
	}
	if len(usable) < 2 {
		t.Fatalf("need two usable nodes, have %d", len(usable))
	}
	picked, other := usable[0], usable[1]
	names, _ := json.Marshal([]string{other.Name})
	doc := fmt.Sprintf(`{
	  "groups": [
	    {"name": "测试组", "type": "select", "nodes": %s},
	    {"name": "自动", "type": "urltest", "filter": "."}
	  ],
	  "rules": [
	    {"domain": ["www.cloudflare.com"], "outbound": "测试组"},
	    {"domain": ["one.one.one.one"], "outbound": "direct"}
	  ],
	  "final": "proxy"
	}`, names)
	engine := New("warning")
	mixed := testserver.FreeTCPPort()
	st, err := engine.Start(StartRequest{
		Link: picked.Link, Name: picked.Name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingRule, Port: mixed,
		RulesJSON: doc, Nodes: usable,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Stop()
	t.Logf("picked %q, 测试组 → %q, groups %d", picked.Name, st.Groups[0].Selected, len(st.Groups))

	viaGroup, err := fetchVia(mixed, "https://www.cloudflare.com/cdn-cgi/trace")
	if err != nil {
		t.Fatal(err)
	}
	viaDirect, err := fetchVia(mixed, "https://one.one.one.one/cdn-cgi/trace")
	if err != nil {
		t.Fatal(err)
	}
	// The picked node's own egress, for comparison: the final is proxy.
	viaPicked, err := fetchVia(mixed, "https://cloudflare.com/cdn-cgi/trace")
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("egress via 测试组 (%s): %s; direct: %s; via the picked node: %s", other.Name, viaGroup, viaDirect, viaPicked)
	outbounds := map[string]string{}
	conns := engine.tracker.Connections()
	for _, c := range append(conns["active"].([]tracker.Conn), conns["closed"].([]tracker.Conn)...) {
		outbounds[c.Host] = c.Outbound
	}
	for host, want := range map[string]string{
		"www.cloudflare.com": route.NodeTag(other.Name),
		"one.one.one.one":    "direct",
		"cloudflare.com":     "proxy",
	} {
		if outbounds[host] != want {
			t.Errorf("%s left by %q, want %q (%v)", host, outbounds[host], want, outbounds)
		}
	}
	if viaGroup == viaDirect {
		t.Errorf("the group's egress equals the direct one: %s", viaGroup)
	}
	groups, err := engine.TestGroup(context.Background(), "自动")
	if err != nil {
		t.Fatal(err)
	}
	out, _ := json.Marshal(groups[1])
	t.Logf("自动 after a test: %s", out)
}

// TestLoopbackLinkExtensions runs fragment (with tfo) on the loopback Vision
// node, as the picked node and as a group's node. (mux has no loopback check:
// Xray's XHTTP inbound refuses TCP inside mux, and Vision servers too.)
func TestLoopbackLinkExtensions(t *testing.T) {
	target, links := startTestServer(t)
	frag := strings.Replace(links[0], "#reality-vision", "&fragment=1-3,1-2,tlshello&tfo=1#frag", 1)
	plain := strings.Replace(links[1], "#reality-xhttp", "#xhttp", 1)
	nodes := []NamedLink{{Name: "frag", Link: frag}, {Name: "xhttp", Link: plain}}
	doc := `{
	  "groups": [{"name": "手选", "type": "select", "nodes": ["frag", "xhttp"]}],
	  "apps": [{"app": "e2e", "outbound": "手选", "match": {"domain_suffix": ["e2e.test"]}}]
	}`
	for _, c := range []struct {
		routing, picked, pick, outbound, dialer string
	}{
		{xrayconf.RoutingGlobal, frag, "", "proxy", "proxy>fragment"},
		{xrayconf.RoutingRule, plain, "frag", route.NodeTag("frag"), "node:frag>fragment"},
		{xrayconf.RoutingRule, frag, "xhttp", route.NodeTag("xhttp"), ""},
	} {
		engine := New("warning")
		mixed := testserver.FreeTCPPort()
		if _, err := engine.Start(StartRequest{
			Link: c.picked, Name: "picked", Mode: xrayconf.ModeProxy, Routing: c.routing, Port: mixed,
			RulesJSON: doc, Nodes: nodes, Selections: map[string]string{"手选": c.pick},
		}); err != nil {
			t.Fatal(err)
		}
		for _, scheme := range []string{"http", "socks5h"} {
			if body := get(t, scheme, mixed, target); body != "hello through the node" {
				t.Fatalf("%+v %s: body %q", c, scheme, body)
			}
		}
		conns := engine.tracker.Connections()
		for _, conn := range append(conns["active"].([]tracker.Conn), conns["closed"].([]tracker.Conn)...) {
			if conn.Outbound != c.outbound {
				t.Fatalf("%+v: connection %+v", c, conn)
			}
		}
		// The node really dialled through its fragment outbound.
		if c.dialer != "" {
			m := engine.inst.GetFeature(stats.ManagerType()).(stats.Manager)
			if up := m.GetCounter("outbound>>>" + c.dialer + ">>>traffic>>>uplink"); up == nil || up.Value() == 0 {
				t.Fatalf("%+v: nothing went through %s", c, c.dialer)
			}
		}
		st := engine.Stop()
		t.Logf("%s via %s: up %d B down %d B", c.routing, c.outbound, st.Up, st.Down)
	}
}

// TestLoopbackChain carries real traffic through a node behind front nodes
// (前置代理), and shows the front is really dialed: a dead one breaks the node.
func TestLoopbackChain(t *testing.T) {
	target, links, others := startTestServerAll(t)
	byName := map[string]string{}
	for _, n := range link.ParseSubscription(strings.Join(append(links, others...), "\n")) {
		byName[n.Name] = n.Link
	}
	u, _ := url.Parse(target)
	base := probe.Options{URL: "http://" + u.Host + "/generate_204"}
	for name, via := range map[string][]string{
		"vision via ss":          {byName["ss"]},
		"xhttp via vmess via ss": {byName["vmess"], byName["ss"]},
		"trojan via vision":      {byName["reality-vision"]},
	} {
		t.Run(name, func(t *testing.T) {
			l := byName["reality-vision"]
			switch {
			case strings.HasPrefix(name, "xhttp"):
				l = byName["reality-xhttp"]
			case strings.HasPrefix(name, "trojan"):
				l = byName["trojan"]
			}
			engine := New("warning")
			mixed := testserver.FreeTCPPort()
			if _, err := engine.Start(StartRequest{Link: l, Name: name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingGlobal, Port: mixed, Via: via}); err != nil {
				t.Fatal(err)
			}
			defer engine.Stop()
			for _, scheme := range []string{"http", "socks5h"} {
				if body := get(t, scheme, mixed, target); body != "hello through the node" {
					t.Fatalf("%s proxy body %q", scheme, body)
				}
			}
			if st := engine.Status(); st.Up == 0 || st.Down == 0 {
				t.Fatalf("status %+v", st)
			}
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			// tcp would skip the proxy; a chained node is always measured end to end.
			opts := base
			opts.Method, opts.Via = "tcp", via
			ms, err := probe.Node(ctx, l, opts)
			if err != nil {
				t.Fatalf("probe: %v", err)
			}
			t.Logf("%s: probe=%dms", name, ms)
		})
	}

	dead := fmt.Sprintf("ss://%s@127.0.0.1:%d#dead", base64.RawURLEncoding.EncodeToString([]byte("aes-256-gcm:pw")), testserver.FreeTCPPort())
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	opts := base
	opts.Via = []string{dead}
	if _, err := probe.Node(ctx, byName["reality-vision"], opts); err == nil {
		t.Fatal("a dead front node must break the chain")
	}
}
