package rulespec

import (
	"encoding/json"
	"errors"
	"testing"

	"github.com/stretchr/testify/require"
)

func parseRule(t *testing.T, text string) Rule {
	t.Helper()
	var rule Rule
	require.NoError(t, json.Unmarshal([]byte(text), &rule))
	return rule
}

func TestRuleNormalizesAndRoundTrips(t *testing.T) {
	rule := parseRule(t, `{
		"domain": "API.Example.com.",
		"domain_suffix": [".Sub.Example.com", "example.org", "example.org", " "],
		"ip_cidr": ["10.0.0.9/8", "2001:db8::1"],
		"port": 443,
		"port_range": ["1000:2000", ":80"],
		"network": "TCP",
		"rule_set": ["Geosite-CN"],
		"outbound": " proxy "
	}`)
	require.Equal(t, ActionRoute, rule.Action)
	require.Equal(t, "proxy", rule.Outbound)
	require.Equal(t, []string{"api.example.com"}, rule.Domain)
	require.Equal(t, []string{".sub.example.com", "example.org"}, rule.DomainSuffix)
	require.Equal(t, []string{"10.0.0.0/8", "2001:db8::1/128"}, rule.IPCIDR)
	require.Equal(t, []uint16{443}, rule.Port)
	require.Equal(t, []string{"tcp"}, rule.Network)
	require.Equal(t, []string{"geosite-cn"}, rule.RuleSets())

	data, err := json.Marshal(rule)
	require.NoError(t, err)
	require.JSONEq(t, `{
		"domain": ["api.example.com"],
		"domain_suffix": [".sub.example.com", "example.org"],
		"ip_cidr": ["10.0.0.0/8", "2001:db8::1/128"],
		"port": [443],
		"port_range": ["1000:2000", ":80"],
		"network": ["tcp"],
		"rule_set": ["geosite-cn"],
		"action": "route",
		"outbound": "proxy"
	}`, string(data))
	require.Equal(t, rule, parseRule(t, string(data)))
}

func TestLogicalRule(t *testing.T) {
	rule := parseRule(t, `{
		"type": "logical", "mode": "AND", "invert": true,
		"rules": [
			{"protocol": "quic"},
			{"type": "logical", "mode": "or", "rules": [{"rule_set": "geosite-youtube"}, {"domain_keyword": "googlevideo"}]}
		],
		"action": "reject"
	}`)
	require.True(t, rule.IsLogical())
	require.Equal(t, ModeAnd, rule.Mode)
	require.True(t, rule.Invert)
	require.Len(t, rule.Rules, 2)
	require.Equal(t, []string{"geosite-youtube"}, rule.RuleSets())
	require.Equal(t, ActionReject, rule.Action)
	require.Equal(t, []string{"protocol"}, Unsupported(rule.Match, Mihomo))
	require.Empty(t, Unsupported(rule.Match, SingBox))
	object := rule.Map()
	require.Equal(t, "logical", object["type"])
	require.Len(t, object["rules"], 2)
}

func TestRuleErrors(t *testing.T) {
	for text, path := range map[string]string{
		`[]`:                     "",
		`{"outbound": "direct"}`: "",
		`{"domain": "a.com"}`:    "outbound",
		`{"domain": "a.com", "action": "reject", "outbound": "x"}`:                                            "outbound",
		`{"domain": "a.com", "action": "sniff"}`:                                                              "action",
		`{"domain": "*.a.com", "outbound": "x"}`:                                                              "domain",
		`{"domain": "bücher.de", "outbound": "x"}`:                                                            "domain",
		`{"domain": ".a.com", "outbound": "x"}`:                                                               "domain",
		`{"domain_regex": "(", "outbound": "x"}`:                                                              "domain_regex",
		`{"ip_cidr": "999.1.1.1", "outbound": "x"}`:                                                           "ip_cidr",
		`{"port": 0, "outbound": "x"}`:                                                                        "port",
		`{"port": "443", "outbound": "x"}`:                                                                    "port",
		`{"port_range": "2000:1000", "outbound": "x"}`:                                                        "port_range",
		`{"port_range": "80", "outbound": "x"}`:                                                               "port_range",
		`{"network": "icmp", "outbound": "x"}`:                                                                "network",
		`{"protocol": "smtp", "outbound": "x"}`:                                                               "protocol",
		`{"rule_set": "Geo Site", "outbound": "x"}`:                                                           "rule_set",
		`{"process_name": "x", "outbound": "x"}`:                                                              "process_name",
		`{"bogus": 1, "domain": "a.com", "outbound": "x"}`:                                                    "bogus",
		`{"mode": "and", "outbound": "x"}`:                                                                    "mode",
		`{"type": "logical", "rules": [{"domain": "a"}], "outbound": "x"}`:                                    "mode",
		`{"type": "logical", "mode": "and", "rules": [], "outbound": "x"}`:                                    "rules",
		`{"type": "logical", "mode": "and", "domain": "a", "rules": [{"domain": "a"}], "outbound": "x"}`:      "domain",
		`{"type": "logical", "mode": "and", "rules": [{"domain": "a", "action": "reject"}], "outbound": "x"}`: "rules[0].action",
		`{"type": "logical", "mode": "and", "rules": [{}], "outbound": "x"}`:                                  "rules[0]",
		`{"type": "logical", "mode": "and", "rules": [{"type": "logical", "mode": "or", "rules": [{"type": "logical", "mode": "or", "rules": [{"domain": "a"}]}]}]}`: "rules[0].rules[0].rules",
	} {
		var rule Rule
		err := json.Unmarshal([]byte(text), &rule)
		var specErr *Error
		require.True(t, errors.As(err, &specErr), "%s: %v", text, err)
		require.Equal(t, path, specErr.Path, "%s: %v", text, err)
	}
}

func TestValueLimit(t *testing.T) {
	ports := make([]int, MaxValues+1)
	for i := range ports {
		ports[i] = i + 1
	}
	data, err := json.Marshal(map[string]any{"port": ports, "outbound": "x"})
	require.NoError(t, err)
	var rule Rule
	require.ErrorContains(t, json.Unmarshal(data, &rule), "最多")
}

func TestUnsupportedMihomoRegex(t *testing.T) {
	rule := parseRule(t, `{"domain_regex": ["^a(b|c)\\.com$"], "outbound": "x"}`)
	require.Equal(t, []string{"domain_regex"}, Unsupported(rule.Match, Mihomo))
	require.Empty(t, Unsupported(parseRule(t, `{"domain_regex": "^ab\\.com$", "outbound": "x"}`).Match, Mihomo))
}
