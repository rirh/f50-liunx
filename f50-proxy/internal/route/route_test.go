package route

import (
	"strings"
	"testing"
)

func TestCompileRouteAcceptsConfigRouteOrArray(t *testing.T) {
	for name, text := range map[string]string{
		"whole config": `{"outbounds": [{"tag": "direct", "type": "direct"}], "route": {"rules": [{"domain_suffix": "x.com", "outbound": "direct"}], "final": "direct"}}`,
		"route":        `{"rules": [{"domain_suffix": "x.com", "outbound": "direct"}], "final": "direct"}`,
		"array":        `[{"domain_suffix": "x.com", "outbound": "direct"}]`,
	} {
		route, err := CompileRoute(text, nil)
		if err != nil || route.Count != 1 {
			t.Errorf("%s: %+v %v", name, route, err)
		}
	}
	if route, err := CompileRoute("  ", nil); err != nil || route.Count != 0 || route.Final != "proxy" {
		t.Errorf("empty text: %+v %v", route, err)
	}
}

func TestCompileRouteNamesErrors(t *testing.T) {
	for text, want := range map[string]string{
		"{\n  \"rules\": [\n    {\"domain\": \"x.com\",}\n  ]\n}":                                                 "JSON 第 3 行",
		`{"rules": [{"domain": "ok.com", "outbound": "direct"}, {"ip_cidr": "300.1.1.1", "outbound": "direct"}]}`: "第 2 条：ip_cidr：「300.1.1.1」不是有效的 IP",
		`{"rules": [{"domain_suffix": "https://x.com/", "outbound": "direct"}]}`:                                  "不是有效的域名",
		`{"rules": [{"domain_regex": "(", "outbound": "direct"}]}`:                                                "不是有效的正则",
		`{"rules": [{"domain": "x.com"}]}`:                                                                        "缺少 outbound",
		`{"rules": [{"outbound": "direct"}]}`:                                                                     "至少需要一个匹配条件",
		`{"rules": [{"port_range": "9:1", "outbound": "direct"}]}`:                                                "port_range",
		`{"rules": [{"type": "logical", "mode": "xor", "rules": [{"port": 1}], "outbound": "direct"}]}`:           "只支持 and 和 or",
		`{"rules": "nope"}`: "类型不对",
	} {
		_, err := CompileRoute(text, nil)
		if err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%s\n want %q, got %v", text, want, err)
		}
	}
}

// AppHost takes the host and names under it, nothing that only ends alike.
func TestAppHost(t *testing.T) {
	p := &Policy{AppHosts: []string{"marseo.eu.org"}}
	for host, want := range map[string]bool{"marseo.eu.org": true, "api.marseo.eu.org": true, "xmarseo.eu.org": false, "eu.org": false} {
		if got := p.AppHost(host); got != want {
			t.Errorf("appHost(%q) = %v, want %v", host, got, want)
		}
	}
}
