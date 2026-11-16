package engine

import (
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"testing"
	"time"

	"f50/proxy/internal/link"
	"f50/proxy/internal/probe"
	"f50/proxy/internal/route"
	"f50/proxy/internal/testserver"
	"f50/proxy/internal/tracker"
	"f50/proxy/internal/xrayconf"
)

// keepAliveClient reuses one connection through the mixed port, as a browser
// keeps an HTTP/2 or keep-alive connection open across page loads.
func keepAliveClient(port int) *http.Client {
	proxy := &url.URL{Scheme: "socks5h", Host: fmt.Sprintf("127.0.0.1:%d", port)}
	return &http.Client{Timeout: 10 * time.Second, Transport: &http.Transport{Proxy: http.ProxyURL(proxy), MaxIdleConnsPerHost: 1}}
}

func fetch(t *testing.T, client *http.Client, target string) {
	t.Helper()
	resp, err := client.Get(target)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if b, _ := io.ReadAll(resp.Body); string(b) != "hello through the node" {
		t.Fatalf("body %q", b)
	}
}

// openTo is the open connections to host.
func openTo(e *Engine, host string) []tracker.Conn {
	var out []tracker.Conn
	for _, c := range e.tracker.Connections()["active"].([]tracker.Conn) {
		if c.Host == host {
			out = append(out, c)
		}
	}
	return out
}

// waitClosed waits until connection id is no longer open.
func waitClosed(t *testing.T, e *Engine, id uint64) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
		open := false
		for _, c := range e.tracker.Connections()["active"].([]tracker.Conn) {
			open = open || c.ID == id
		}
		if !open {
			return
		}
	}
	t.Fatalf("connection %d still open", id)
}

func loopbackNodes(t *testing.T) (target, host string, nodes []NamedLink) {
	t.Helper()
	target, links := startTestServer(t)
	u, _ := url.Parse(target)
	probe.Host = u.Hostname()
	probe.Port, _ = strconv.Atoi(u.Port())
	for _, n := range link.ParseSubscription(strings.Join(links, "\n")) {
		nodes = append(nodes, NamedLink{Name: n.Name, Link: n.Link})
	}
	if len(nodes) != 2 {
		t.Fatalf("nodes %+v", nodes)
	}
	return target, u.Hostname(), nodes
}

// TestSelectGroupClosesOldFlows: a connection the group sent to its old node
// is closed by the pick, so the same keep-alive client's next request leaves
// by the new node; a connection outside the group stays open.
func TestSelectGroupClosesOldFlows(t *testing.T) {
	target, host, nodes := loopbackNodes(t)
	doc := `{
	  "groups": [{"name": "手选", "type": "select", "nodes": ["reality-vision", "reality-xhttp"]}],
	  "apps": [{"app": "e2e", "outbound": "手选", "match": {"domain_suffix": ["e2e.test"]}}],
	  "final": "reality-vision"
	}`
	engine := New("warning")
	mixed := testserver.FreeTCPPort()
	if _, err := engine.Start(StartRequest{
		Link: nodes[0].Link, Name: nodes[0].Name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingRule, Port: mixed,
		RulesJSON: doc, Nodes: nodes, Selections: map[string]string{"手选": "reality-xhttp"},
	}); err != nil {
		t.Fatal(err)
	}
	defer engine.Stop()

	client := keepAliveClient(mixed)
	fetch(t, client, target)
	before := openTo(engine, host)
	if len(before) != 1 || before[0].Outbound != route.NodeTag("reality-xhttp") || before[0].Group != "手选" {
		t.Fatalf("before the pick: %+v", before)
	}
	// Outside the group: the final node, by an IP the rule does not name.
	other := keepAliveClient(mixed)
	fetch(t, other, strings.Replace(target, host, "127.0.0.1", 1))
	outside := openTo(engine, "127.0.0.1")
	if len(outside) != 1 || outside[0].Group != "" {
		t.Fatalf("outside the group: %+v", outside)
	}

	if _, err := engine.SelectGroup("手选", "reality-vision"); err != nil {
		t.Fatal(err)
	}
	waitClosed(t, engine, before[0].ID)
	fetch(t, client, target)
	after := openTo(engine, host)
	if len(after) != 1 || after[0].ID == before[0].ID || after[0].Outbound != route.NodeTag("reality-vision") {
		t.Fatalf("after the pick: %+v", after)
	}
	if still := openTo(engine, "127.0.0.1"); len(still) != 1 || still[0].ID != outside[0].ID {
		t.Fatalf("the pick closed a connection outside the group: %+v", still)
	}

	// Picking the node the group is already on closes nothing.
	if _, err := engine.SelectGroup("手选", "reality-vision"); err != nil {
		t.Fatal(err)
	}
	time.Sleep(100 * time.Millisecond)
	if again := openTo(engine, host); len(again) != 1 || again[0].ID != after[0].ID {
		t.Fatalf("re-picking the same node: %+v", again)
	}
}

// TestRestartClosesOldFlows: switching the node restarts the instance, and
// every connection the old one carried is closed, so the keep-alive client's
// next request goes through the new node.
func TestRestartClosesOldFlows(t *testing.T) {
	target, host, nodes := loopbackNodes(t)
	engine := New("warning")
	mixed := testserver.FreeTCPPort()
	start := func(n NamedLink) {
		if _, err := engine.Start(StartRequest{Link: n.Link, Name: n.Name, Mode: xrayconf.ModeProxy, Routing: xrayconf.RoutingGlobal, Port: mixed}); err != nil {
			t.Fatal(err)
		}
	}
	start(nodes[0])
	defer engine.Stop()

	client := keepAliveClient(mixed)
	fetch(t, client, target)
	before := openTo(engine, host)
	if len(before) != 1 {
		t.Fatalf("before the switch: %+v", before)
	}
	start(nodes[1])
	if open := openTo(engine, host); len(open) != 0 {
		t.Fatalf("the old instance's connections are still open: %+v", open)
	}
	fetch(t, client, target)
	after := openTo(engine, host)
	if len(after) != 1 || after[0].ID == before[0].ID || engine.Status().Node != nodes[1].Name {
		t.Fatalf("after the switch: %+v", after)
	}
}
