package engine

import (
	"context"
	"errors"
	"testing"

	"f50/proxy/internal/route"
)

func TestPickURLTest(t *testing.T) {
	nodes := []string{"a", "b", "c"}
	for _, c := range []struct {
		delays  map[string]delay
		current string
		want    string
	}{
		{map[string]delay{}, "a", "a"},                                   // nothing tested yet
		{map[string]delay{"a": {0}, "b": {0}, "c": {0}}, "b", "b"},       // all failed: stay
		{map[string]delay{"a": {300}, "b": {120}, "c": {200}}, "a", "b"}, // clearly faster
		{map[string]delay{"a": {150}, "b": {120}, "c": {200}}, "a", "a"}, // within the tolerance: no flapping
		{map[string]delay{"a": {171}, "b": {120}}, "a", "b"},             // beyond it
		{map[string]delay{"a": {0}, "b": {400}, "c": {390}}, "a", "c"},   // the current one failed
		{map[string]delay{"b": {100}}, "gone", "b"},                      // current no longer a member
		{map[string]delay{"a": {100}, "b": {100}, "c": {100}}, "c", "c"}, // ties stay
		{map[string]delay{"a": {100}, "b": {100}, "c": {100}}, "", "a"},  // first of the fastest
		{map[string]delay{"a": {100}, "c": {40}}, "a", "c"},              // 60 ms faster
	} {
		if got := pickURLTest(nodes, c.delays, c.current); got != c.want {
			t.Errorf("%v from %q: %s, want %s", c.delays, c.current, got, c.want)
		}
	}
}

func TestPickFallback(t *testing.T) {
	nodes := []string{"a", "b", "c"}
	for _, c := range []struct {
		delays map[string]delay
		want   string
	}{
		{map[string]delay{}, "a"},
		{map[string]delay{"a": {0}}, "b"},
		{map[string]delay{"a": {0}, "b": {900}, "c": {10}}, "b"}, // order, not speed
		{map[string]delay{"a": {0}, "b": {0}, "c": {0}}, "a"},
	} {
		if got := pickFallback(nodes, c.delays); got != c.want {
			t.Errorf("%v: %s, want %s", c.delays, got, c.want)
		}
	}
}

// The group set as the router and the shell use it: saved picks, manual
// selection, tests that re-pick and the status the UI shows.
func TestGroupSet(t *testing.T) {
	rt, err := route.CompileRoute(groupsDoc, nil, pcNodes...)
	if err != nil {
		t.Fatal(err)
	}
	set := newGroupSet(rt.Groups, map[string]string{"美国": "US-2", "保底": "US-1", "自动": "US-2"})
	delays := map[string]int64{"US-1": 0, "US-2": 210, "美国 LA": 90, "HK-1": 0}
	set.test = func(ctx context.Context, node string) (int64, error) {
		if ms := delays[node]; ms > 0 {
			return ms, nil
		}
		return 0, errors.New("timeout")
	}
	// Saved picks hold for select groups only.
	if got := set.resolve(route.GroupTag("美国")); got != route.NodeTag("US-2") {
		t.Fatalf("美国 → %s", got)
	}
	if got := set.resolve(route.GroupTag("自动")); got != route.NodeTag("US-1") {
		t.Fatalf("自动 starts on its first node, got %s", got)
	}
	if got := set.resolve("direct"); got != "direct" {
		t.Fatal(got)
	}
	if _, err := set.Select("美国", "HK-1"); err == nil {
		t.Fatal("a node outside the group was accepted")
	}
	if _, err := set.Select("自动", "US-2"); err == nil {
		t.Fatal("urltest accepted a manual pick")
	}
	if before, err := set.Select("美国", "US-1"); err != nil || before != "US-2" {
		t.Fatal(before, err)
	}
	if err := set.Test(context.Background(), "自动"); err != nil {
		t.Fatal(err)
	}
	if err := set.Test(context.Background(), "保底"); err != nil {
		t.Fatal(err)
	}
	if err := set.Test(context.Background(), "nope"); err == nil {
		t.Fatal("unknown group tested")
	}
	status := set.Status()
	if status[0].Selected != "US-1" || status[1].Selected != "美国 LA" {
		t.Fatalf("status %+v", status)
	}
	// 保底: HK-1 and US-1 both failed, so its first node.
	if status[2].Selected != "HK-1" {
		t.Fatalf("all of 保底 failed, so its first node: %+v", status[2])
	}
	la := status[1].Nodes[2]
	if la.Name != "美国 LA" || la.DelayMs == nil || *la.DelayMs != 90 || !la.Alive {
		t.Fatalf("美国 LA %+v", la)
	}
	us1 := status[1].Nodes[0]
	if us1.DelayMs != nil || us1.Alive {
		t.Fatalf("US-1 %+v", us1)
	}
	// A select group's members show the delays other groups measured.
	if status[0].Nodes[1].DelayMs == nil || *status[0].Nodes[1].DelayMs != 210 {
		t.Fatalf("美国 %+v", status[0].Nodes)
	}

	// HK-1 comes back: fallback returns to it.
	delays["HK-1"] = 500
	_ = set.Test(context.Background(), "保底")
	if got := set.resolve(route.GroupTag("保底")); got != route.NodeTag("HK-1") {
		t.Fatalf("保底 → %s", got)
	}
	// 美国 LA slows a little: urltest stays; a lot: it moves.
	delays["美国 LA"] = 230
	_ = set.Test(context.Background(), "自动")
	if got := set.resolve(route.GroupTag("自动")); got != route.NodeTag("美国 LA") {
		t.Fatalf("自动 flapped to %s", got)
	}
	delays["美国 LA"] = 400
	_ = set.Test(context.Background(), "自动")
	if got := set.resolve(route.GroupTag("自动")); got != route.NodeTag("US-2") {
		t.Fatalf("自动 → %s", got)
	}
}

// pcNodes and groupsDoc: a PC-style document with every kind of group (as in router).
var pcNodes = []string{"US-1", "US-2", "HK-1", "美国 LA", "JP-1"}

const groupsDoc = `{
  "groups": [
    {"name": "美国", "type": "select", "nodes": ["US-1", "US-2"]},
    {"name": "自动", "type": "urltest", "filter": "(?i)us|美国", "interval": 30},
    {"name": "保底", "type": "fallback", "nodes": ["HK-1", "US-1", "Gone"]},
    {"name": "空", "type": "select", "nodes": ["Gone"], "filter": "^none$"}
  ],
  "apps": [
    {"app": "openai", "outbound": "美国", "match": {"domain_suffix": ["openai.com"]}},
    {"app": "telegram", "outbound": "自动", "match": {"domain_suffix": ["t.me"], "ip_cidr": ["91.108.4.0/22"]}},
    {"app": "broken", "outbound": "空", "match": {"domain": ["broken.example"]}},
    {"app": "missing", "outbound": "没有这个组", "match": {"domain": ["missing.example"]}},
    {"app": "corp-app", "outbound": "JP-1", "match": {"domain_suffix": ["corp.example.com"]}}
  ],
  "rules": [
    {"domain_suffix": ["corp.example.com"], "outbound": "direct"},
    {"domain": ["hk.example"], "outbound": "保底"},
    {"domain": ["ghost.example"], "outbound": "Ghost"},
    {"domain": ["ads.example"], "action": "reject"}
  ],
  "final": "自动"
}`
