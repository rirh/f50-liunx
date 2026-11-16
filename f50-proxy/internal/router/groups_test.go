package router

import (
	"net/netip"
	"reflect"
	"strings"
	"testing"

	"f50/proxy/internal/route"
	"f50/proxy/internal/rulespec/engine"
	"f50/proxy/internal/xrayconf"
)

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

func TestCompileRouteGroupsAndApps(t *testing.T) {
	rt, err := route.CompileRoute(groupsDoc, nil, pcNodes...)
	if err != nil {
		t.Fatal(err)
	}
	want := []route.Group{
		{Name: "美国", Type: route.GroupSelect, Nodes: []string{"US-1", "US-2"}},
		// filter matches in the node list's order; interval raised to the minimum
		{Name: "自动", Type: route.GroupURLTest, Nodes: []string{"US-1", "US-2", "美国 LA"}, Interval: 60},
		{Name: "保底", Type: route.GroupFallback, Nodes: []string{"HK-1", "US-1"}, Interval: 300},
	}
	if !reflect.DeepEqual(rt.Groups, want) {
		t.Fatalf("groups %+v", rt.Groups)
	}
	if !reflect.DeepEqual(rt.Nodes, []string{"US-1", "US-2", "美国 LA", "HK-1", "JP-1"}) {
		t.Fatalf("nodes %v", rt.Nodes)
	}
	if rt.Final != "自动" || rt.FinalTag() != route.GroupTag("自动") {
		t.Fatalf("final %q %q", rt.Final, rt.FinalTag())
	}
	// 4 rules - Ghost + 5 apps - 空 - 没有这个组
	if rt.Source != 6 {
		t.Fatalf("source %d", rt.Source)
	}
	warnings := strings.Join(rt.Warnings, "\n")
	for _, want := range []string{"interval", "节点 Gone 不存在", "策略组 空：没有可用节点", "Ghost", "应用 broken", "应用 missing"} {
		if !strings.Contains(warnings, want) {
			t.Errorf("warnings lack %q:\n%s", want, warnings)
		}
	}
	if strings.Contains(warnings, "当前选中的节点") {
		t.Errorf("a document with groups must not send unknown outbounds to the picked node:\n%s", warnings)
	}

	for _, c := range []struct {
		conn engine.Conn
		want string
	}{
		// rules before apps: corp.example.com stays direct although an app sends it to JP-1
		{engine.Conn{Domain: "git.corp.example.com", Network: "tcp", Port: 443}, "direct"},
		{engine.Conn{Domain: "hk.example", Network: "tcp", Port: 443}, route.GroupTag("保底")},
		{engine.Conn{Domain: "ads.example", Network: "tcp", Port: 443}, "block"},
		{engine.Conn{Domain: "api.openai.com", Network: "tcp", Port: 443}, route.GroupTag("美国")},
		{engine.Conn{IP: netip.MustParseAddr("91.108.4.10"), Network: "tcp", Port: 443}, route.GroupTag("自动")},
		// skipped rules fall through: to the final (a group), via the overseas list or not
		{engine.Conn{Domain: "ghost.example", Network: "tcp", Port: 443}, route.GroupTag("自动")},
		{engine.Conn{Domain: "broken.example", Network: "tcp", Port: 443}, route.GroupTag("自动")},
		{engine.Conn{Domain: "www.google.com", Network: "tcp", Port: 443}, route.GroupTag("自动")},
		// the mainland tail still runs before the final
		{engine.Conn{Domain: "www.baidu.com", Network: "tcp", Port: 443}, "direct"},
		{engine.Conn{IP: netip.MustParseAddr("192.168.1.10"), Network: "tcp", Port: 22}, "direct"},
	} {
		if got := decide(t, xrayconf.RoutingRule, rt, c.conn); got != c.want {
			t.Errorf("%+v: %s, want %s", c.conn, got, c.want)
		}
	}
}

// Without groups or apps a document reads as before, apart from names that
// are PC nodes.
func TestCompileRouteOldDocuments(t *testing.T) {
	text := `{"rules": [{"domain": ["a.example"], "outbound": "🚀 节点选择"}, {"domain": ["b.example"], "outbound": "HK-1"}], "final": "Other"}`
	rt, err := route.CompileRoute(text, nil, pcNodes...)
	if err != nil {
		t.Fatal(err)
	}
	if rt.Final != "proxy" || rt.FinalTag() != "" || len(rt.Groups) != 0 {
		t.Fatalf("final %q %q groups %v", rt.Final, rt.FinalTag(), rt.Groups)
	}
	if !strings.Contains(strings.Join(rt.Warnings, "\n"), "Other、🚀 节点选择 都按当前选中的节点处理") {
		t.Fatalf("warnings %v", rt.Warnings)
	}
	if got := decide(t, xrayconf.RoutingRule, rt, engine.Conn{Domain: "a.example", Network: "tcp", Port: 443}); got != "proxy" {
		t.Fatalf("a.example: %s", got)
	}
	if got := decide(t, xrayconf.RoutingRule, rt, engine.Conn{Domain: "b.example", Network: "tcp", Port: 443}); got != route.NodeTag("HK-1") {
		t.Fatalf("b.example: %s", got)
	}
	if !reflect.DeepEqual(rt.Nodes, []string{"HK-1"}) {
		t.Fatalf("nodes %v", rt.Nodes)
	}
	// Without the node list (the old callers) every name is the picked node.
	rt, err = route.CompileRoute(text, nil)
	if err != nil || len(rt.Nodes) != 0 {
		t.Fatalf("%v %v", err, rt.Nodes)
	}
}

func TestCompileRouteGroupErrors(t *testing.T) {
	for text, want := range map[string]string{
		`{"groups": [{"type": "select", "nodes": ["US-1"]}]}`:                                                    "策略组第 1 个：需要 name",
		`{"groups": [{"name": "proxy", "type": "select", "nodes": ["US-1"]}]}`:                                   "组名不能叫 proxy",
		`{"groups": [{"name": "a", "type": "select", "nodes": ["US-1"]}, {"name": "a", "type": "select"}]}`:      "组名 a 重复",
		`{"groups": [{"name": "a", "type": "load-balance", "nodes": ["US-1"]}]}`:                                 "策略组 a：type",
		`{"groups": [{"name": "a", "type": "urltest", "filter": "(us"}]}`:                                        "filter 不是有效的正则",
		`{"apps": [{"app": "x", "outbound": "direct", "match": {"domain": ["x.example"], "action": "reject"}}]}`: "应用 x：match 不能带 action",
		`{"apps": [{"app": "x", "outbound": "direct"}]}`:                                                         "应用 x：缺少 match",
		`{"apps": [{"app": "x", "match": {"domain": ["x.example"]}}]}`:                                           "应用 x：缺少 outbound",
		`{"apps": [{"app": "x", "outbound": "direct", "match": {"ip_cidr": ["300.1.1.1"]}}]}`:                    "应用 x：",
	} {
		_, err := route.CompileRoute(text, nil, pcNodes...)
		if err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%s: %v, want %q", text, err, want)
		}
	}
	// Soft problems only warn: a group named like a node, a nested group.
	rt, err := route.CompileRoute(`{"groups": [
	  {"name": "HK-1", "type": "select", "nodes": ["US-1"]},
	  {"name": "a", "type": "select", "nodes": ["b", "US-1"]},
	  {"name": "b", "type": "select", "nodes": ["US-2"]}
	], "rules": [{"domain": ["x.example"], "outbound": "HK-1"}]}`, nil, pcNodes...)
	if err != nil {
		t.Fatal(err)
	}
	warnings := strings.Join(rt.Warnings, "\n")
	if !strings.Contains(warnings, "和节点重名") || !strings.Contains(warnings, "不能嵌套策略组 b") {
		t.Fatalf("warnings %s", warnings)
	}
	if len(rt.Groups) != 2 || !reflect.DeepEqual(rt.Groups[0].Nodes, []string{"US-1"}) {
		t.Fatalf("groups %+v", rt.Groups)
	}
	if got := decide(t, xrayconf.RoutingRule, rt, engine.Conn{Domain: "x.example", Network: "tcp", Port: 443}); got != route.NodeTag("HK-1") {
		t.Fatalf("x.example: %s", got)
	}
}
