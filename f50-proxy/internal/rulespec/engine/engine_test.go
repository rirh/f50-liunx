package engine

import (
	"encoding/json"
	"fmt"
	"net/netip"
	"os"
	"testing"

	"github.com/stretchr/testify/require"

	"f50/proxy/internal/rulespec"
)

// testdata/conformance.json pairs rules and connections with the verdicts
// sing-box's own route-rule matcher (route/rule, 1.14) gave for every pair.
type conformance struct {
	RuleSets map[string][]rulespec.Match `json:"rule_sets"`
	Rules    []rulespec.Match            `json:"rules"`
	Conns    []struct {
		Domain   string   `json:"domain"`
		IP       string   `json:"ip"`
		Resolved []string `json:"resolved"`
		Network  string   `json:"network"`
		Port     uint16   `json:"port"`
		Protocol string   `json:"protocol"`
	} `json:"conns"`
	Verdicts [][]bool `json:"verdicts"`
}

func TestMatchesLikeSingBox(t *testing.T) {
	raw, err := os.ReadFile("testdata/conformance.json")
	require.NoError(t, err)
	var data conformance
	require.NoError(t, json.Unmarshal(raw, &data))
	require.Len(t, data.Verdicts, len(data.Rules))
	sets := func(tag string) ([]rulespec.Match, error) {
		rules, ok := data.RuleSets[tag]
		if !ok {
			return nil, fmt.Errorf("unknown")
		}
		return rules, nil
	}
	mismatches := 0
	for i, rule := range data.Rules {
		ruleJSON, _ := json.Marshal(rule)
		for j, spec := range data.Conns {
			conn := &Conn{Domain: spec.Domain, Network: spec.Network, Port: spec.Port, Protocol: spec.Protocol}
			if spec.IP != "" {
				conn.IP = netip.MustParseAddr(spec.IP)
			}
			for _, address := range spec.Resolved {
				conn.Resolved = append(conn.Resolved, netip.MustParseAddr(address))
			}
			got, err := Match(rule, sets, conn)
			require.NoError(t, err)
			if got != data.Verdicts[i][j] {
				mismatches++
				t.Errorf("rule %d %s on conn %d %+v: got %v, sing-box %v", i, ruleJSON, j, spec, got, data.Verdicts[i][j])
			}
		}
	}
	require.Zero(t, mismatches)
}

func TestRouteFirstMatchAndResolve(t *testing.T) {
	var steps []Step
	for _, text := range []string{
		`{"ip_is_private": true, "outbound": "direct"}`,
		`{"domain_suffix": "blocked.example", "action": "reject"}`,
		`{"rule_set": "geosite-cn", "outbound": "direct"}`,
		"resolve",
		`{"rule_set": "geoip-cn", "outbound": "direct"}`,
	} {
		if text == "resolve" {
			steps = append(steps, Step{Resolve: true})
			continue
		}
		var rule rulespec.Rule
		require.NoError(t, json.Unmarshal([]byte(text), &rule))
		steps = append(steps, Step{Rule: rule})
	}
	catalogue := map[string]string{
		"geosite-cn": `{"version": 2, "rules": [{"domain_suffix": ["cn", "qq.com"]}]}`,
		"geoip-cn":   `{"version": 2, "rules": [{"ip_cidr": ["114.114.114.0/24"]}]}`,
	}
	loads := 0
	router, err := New(steps, func(tag string) ([]rulespec.Match, error) {
		loads++
		return ParseRuleSet([]byte(catalogue[tag]))
	})
	require.NoError(t, err)
	require.Equal(t, 2, loads)

	lookups := 0
	resolver := func(addresses ...string) func() []netip.Addr {
		return func() []netip.Addr {
			lookups++
			var out []netip.Addr
			for _, address := range addresses {
				out = append(out, netip.MustParseAddr(address))
			}
			return out
		}
	}
	route := func(conn *Conn, resolve func() []netip.Addr) Result { return router.Route(conn, resolve) }

	require.Equal(t, Result{Matched: true, Index: 2, Action: "route", Outbound: "direct"}, route(&Conn{Domain: "www.qq.com", Network: "tcp", Port: 443}, resolver()))
	require.Equal(t, Result{Matched: true, Index: 1, Action: "reject"}, route(&Conn{Domain: "a.blocked.example"}, resolver()))
	require.Zero(t, lookups, "domain rules decide before the resolve step")
	// An overseas-looking name on a mainland address is caught after resolve.
	require.Equal(t, Result{Matched: true, Index: 4, Action: "route", Outbound: "direct"}, route(&Conn{Domain: "dns.example"}, resolver("114.114.114.114")))
	require.Equal(t, 1, lookups)
	require.Equal(t, Result{}, route(&Conn{Domain: "example.org"}, resolver("93.184.216.34")))
	require.Equal(t, Result{Matched: true, Index: 0, Action: "route", Outbound: "direct"}, route(&Conn{IP: netip.MustParseAddr("192.168.1.2")}, nil))
}

func TestParseRuleSetTakesLargeSets(t *testing.T) {
	suffixes := make([]string, rulespec.MaxValues+10)
	for i := range suffixes {
		suffixes[i] = fmt.Sprintf("s%d.example", i)
	}
	data, err := json.Marshal(map[string]any{"version": 2, "rules": []any{map[string]any{"domain_suffix": suffixes}}})
	require.NoError(t, err)
	rules, err := ParseRuleSet(data)
	require.NoError(t, err)
	require.Len(t, rules[0].DomainSuffix, len(suffixes))
	_, err = ParseRuleSet([]byte(`{"version": 2, "rules": [{"process_name": ["x"]}]}`))
	require.Error(t, err)
}
