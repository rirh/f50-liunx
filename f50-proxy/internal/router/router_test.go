package router

import (
	"context"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"strings"
	"sync/atomic"
	"testing"

	"f50/proxy/internal/geodata"
	"f50/proxy/internal/route"
	"f50/proxy/internal/rulespec/engine"
	"f50/proxy/internal/xrayconf"

	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/features/routing"
)

// DNS that reaches the tproxy inbound goes to dns-out before any rule; other
// ports and other inbounds do not.
func TestGatewayFixedRoute(t *testing.T) {
	for _, c := range []struct {
		ctx  fakeContext
		want string
	}{
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("114.114.114.114"), port: 53, network: xnet.Network_UDP}, xrayconf.DNSOutTag},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("10.10.10.10"), port: 53, network: xnet.Network_TCP}, xrayconf.DNSOutTag},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("1.1.1.1"), port: 853, network: xnet.Network_TCP}, ""},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("2001:db8::1"), port: 443, network: xnet.Network_TCP}, ""},
		{fakeContext{inbound: "mixed", ip: xnet.ParseIP("1.1.1.1"), port: 53, network: xnet.Network_UDP}, ""},
		{fakeContext{inbound: xrayconf.DNSRemoteTag, ip: xnet.ParseIP("1.1.1.1"), port: 443, network: xnet.Network_TCP}, "proxy"},
	} {
		if got := fixedRoute(c.ctx); got != c.want {
			t.Errorf("%+v: %q, want %q", c.ctx, got, c.want)
		}
	}
}

// decide runs a mode's policy on one connection and returns the outbound.
func decide(t *testing.T, routing string, rt route.Route, conn engine.Conn, resolved ...string) string {
	t.Helper()
	policy, err := route.NewPolicy(routing, rt)
	if err != nil {
		t.Fatal(err)
	}
	result := policy.Router.Route(&conn, func() []netip.Addr {
		var out []netip.Addr
		for _, address := range resolved {
			out = append(out, netip.MustParseAddr(address))
		}
		return out
	})
	if !result.Matched {
		return policy.Final
	}
	return outboundTag(result)
}

func TestCompileRouteFollowsSingBox(t *testing.T) {
	// geosite-example is not embedded (rules/apps): without a catalogue it is skipped.
	rt, err := route.CompileRoute(`{
	  "rules": [
	    { "action": "sniff" },
	    { "protocol": "dns", "action": "hijack-dns" },
	    { "domain": ["Code.Qutke.cn"], "ip_cidr": ["52.81.156.229"], "outbound": "US-1" },
	    { "domain_suffix": [".qutke.com", "qutke.cn"], "domain_keyword": "adservice", "domain_regex": "^ads\\.", "outbound": "direct" },
	    { "rule_set": ["geosite-example", "extra"], "outbound": "proxy" },
	    { "rule_set": "extra", "port": 443, "outbound": "proxy" },
	    { "clash_mode": "Direct", "outbound": "direct" },
	    { "domain_suffix": ["bilibili.com"], "wifi_ssid": ["BJOF-S"], "outbound": "proxy" },
	    { "type": "logical", "mode": "or", "rules": [{ "port": 853 }, { "network": "udp", "port": 443 }, { "protocol": "stun" }], "action": "reject" },
	    { "type": "logical", "mode": "and", "rules": [{ "domain_suffix": "corp.example" }, { "port_range": ["8000:8100", ":22"] }], "outbound": "direct" },
	    { "type": "logical", "mode": "and", "rules": [{ "domain_suffix": "noisy.example", "invert": true }, { "network": "udp", "port": 3478 }], "action": "reject" },
	    { "action": "resolve" },
	    { "ip_cidr": "203.0.113.0/24", "outbound": "proxy" }
	  ],
	  "rule_set": [
	    { "tag": "geosite-example", "type": "remote", "url": "https://example.com/example.srs" },
	    { "tag": "extra", "type": "inline", "rules": [{ "domain": ["persistent.oaistatic.com"], "domain_suffix": ["ip.sb"] }] }
	  ],
	  "final": "direct"
	}`, nil)
	if err != nil {
		t.Fatal(err)
	}
	if rt.Source != 7 || rt.Final != "direct" {
		t.Fatalf("source %d final %s", rt.Source, rt.Final)
	}
	warnings := strings.Join(rt.Warnings, "\n")
	for _, want := range []string{"geosite-example", "clash_mode", "wifi_ssid", "US-1"} {
		if !strings.Contains(warnings, want) {
			t.Errorf("warnings lack %s:\n%s", want, warnings)
		}
	}
	for _, c := range []struct {
		conn     engine.Conn
		resolved []string
		want     string
	}{
		{engine.Conn{Domain: "code.qutke.cn", Network: "tcp", Port: 443}, nil, "proxy"},
		{engine.Conn{Domain: "a.qutke.com", Network: "tcp", Port: 443}, nil, "direct"},
		// ".qutke.com" leaves the bare domain to later rules.
		{engine.Conn{Domain: "qutke.com", Network: "tcp", Port: 80}, nil, "direct"},
		// The inline set's suffix merges with port 443 (rule 6), as in sing-box.
		{engine.Conn{Domain: "api.ip.sb", Network: "tcp", Port: 443}, nil, "proxy"},
		{engine.Conn{Domain: "dns.example", Network: "tcp", Port: 853}, nil, "block"},
		{engine.Conn{Domain: "corp.example", Network: "tcp", Port: 22}, nil, "direct"},
		{engine.Conn{Domain: "talk.example", Network: "udp", Port: 3478}, nil, "block"},
		{engine.Conn{Domain: "noisy.example", Network: "udp", Port: 3478}, nil, "direct"},
		// After the route's resolve step, an IP rule catches a domain.
		{engine.Conn{Domain: "shop.example", Network: "tcp", Port: 443}, []string{"203.0.113.9"}, "proxy"},
		// Mainland lists run after the route, then the final outbound.
		{engine.Conn{Domain: "www.qq.com", Network: "tcp", Port: 443}, nil, "direct"},
		{engine.Conn{Domain: "example.org", Network: "tcp", Port: 443}, []string{"93.184.216.34"}, "direct"},
	} {
		if got := decide(t, xrayconf.RoutingRule, rt, c.conn, c.resolved...); got != c.want {
			t.Errorf("%+v: got %s, want %s", c.conn, got, c.want)
		}
	}
}

func TestPolicyModes(t *testing.T) {
	rt, err := route.CompileRoute(`{"rules": [{"domain_suffix": "qq.com", "outbound": "proxy"}, {"ip_cidr": "8.8.8.0/24", "action": "reject"}], "final": "proxy"}`, nil)
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		conn     engine.Conn
		resolved []string
		modes    map[string]string
	}{
		// The user's rule beats the mainland list in 规则 mode only.
		{engine.Conn{Domain: "www.qq.com"}, nil, map[string]string{xrayconf.RoutingRule: "proxy", xrayconf.RoutingGlobal: "proxy", xrayconf.RoutingDirect: "direct"}},
		{engine.Conn{IP: netip.MustParseAddr("8.8.8.8")}, nil, map[string]string{xrayconf.RoutingRule: "block", xrayconf.RoutingGlobal: "proxy", xrayconf.RoutingDirect: "direct"}},
		{engine.Conn{Domain: "www.baidu.com"}, nil, map[string]string{xrayconf.RoutingRule: "direct", xrayconf.RoutingGlobal: "proxy", xrayconf.RoutingDirect: "direct"}},
		// A name on a mainland address goes direct once resolved.
		{engine.Conn{Domain: "cdn.example"}, []string{"114.114.114.114"}, map[string]string{xrayconf.RoutingRule: "direct", xrayconf.RoutingGlobal: "proxy"}},
		{engine.Conn{IP: netip.MustParseAddr("192.168.1.10")}, nil, map[string]string{xrayconf.RoutingRule: "direct", xrayconf.RoutingGlobal: "direct", xrayconf.RoutingDirect: "direct"}},
		{engine.Conn{Domain: "localhost"}, nil, map[string]string{xrayconf.RoutingGlobal: "direct"}},
		{engine.Conn{Domain: "printer.local"}, nil, map[string]string{xrayconf.RoutingGlobal: "direct", xrayconf.RoutingRule: "direct"}},
		// Known overseas sites reach the node before any lookup: the address the
		// resolver would give is never asked for.
		{engine.Conn{Domain: "www.google.com"}, []string{"114.114.114.114"}, map[string]string{xrayconf.RoutingRule: "proxy"}},
	}
	for _, c := range cases {
		for mode, want := range c.modes {
			if got := decide(t, mode, rt, c.conn, c.resolved...); got != want {
				t.Errorf("%s %+v: got %s, want %s", mode, c.conn, got, want)
			}
		}
	}
}

// With a final of direct (proxy only what the rules name), the overseas list
// must not send anything to the node.
func TestOverseasListFollowsTheFinal(t *testing.T) {
	rt, err := route.CompileRoute(`{"rules": [{"domain_suffix": "openai.com", "outbound": "proxy"}], "final": "direct"}`, nil)
	if err != nil {
		t.Fatal(err)
	}
	for domain, want := range map[string]string{"chatgpt.openai.com": "proxy", "www.google.com": "direct", "www.qq.com": "direct"} {
		if got := decide(t, xrayconf.RoutingRule, rt, engine.Conn{Domain: domain}); got != want {
			t.Errorf("%s: got %s, want %s", domain, got, want)
		}
	}
	sets := geodata.OverseasDomains()
	if len(sets) == 0 || len(sets[0].DomainSuffix) < 10000 {
		t.Fatalf("embedded geolocation-!cn looks empty: %d rules", len(sets))
	}
	if rules, ok := geodata.Builtin("geosite-geolocation-!cn"); !ok || len(rules) != len(sets) {
		t.Error("geosite-geolocation-!cn is not built in")
	}
}

func TestCompileRouteLoadsCatalogueSets(t *testing.T) {
	// A tag that is not embedded (rules/apps), so it comes from the catalogue.
	// (route.TestCatalogueRevalidates covers the cache.)
	var requests, revalidations atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		if r.URL.Path != "/api/v1/one/rules/geosite-example.json" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("ETag", `"v1"`)
		if r.Header.Get("If-None-Match") == `"v1"` {
			revalidations.Add(1)
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Write([]byte(`{"version": 2, "rules": [{"domain_suffix": ["example.com", "example.net"]}]}`))
	}))
	defer server.Close()
	remote := route.CatalogueAt(server.URL + "/api/v1/one/rules")
	text := `[{"rule_set": "geosite-example", "outbound": "proxy"}, {"rule_set": "geosite-missing", "outbound": "direct"}, {"rule_set": "geoip-cn", "outbound": "proxy"}]`
	rt, err := route.CompileRoute(text, remote)
	if err != nil {
		t.Fatal(err)
	}
	if rt.Source != 2 || !strings.Contains(strings.Join(rt.Warnings, ""), "geosite-missing") {
		t.Fatalf("source %d warnings %v", rt.Source, rt.Warnings)
	}
	if got := decide(t, xrayconf.RoutingRule, rt, engine.Conn{Domain: "example.net"}); got != "proxy" {
		t.Errorf("example.net went %s", got)
	}
	// geoip-cn is built in, so a mainland address is sent to the node here.
	if got := decide(t, xrayconf.RoutingRule, rt, engine.Conn{IP: netip.MustParseAddr("114.114.114.114")}); got != "proxy" {
		t.Errorf("geoip-cn went %s", got)
	}
}

// fakeContext is a connection as Xray's dispatcher describes it.
type fakeContext struct {
	routing.Context
	inbound  string
	domain   string
	ip       xnet.IP
	port     xnet.Port
	network  xnet.Network
	protocol string
}

func (c fakeContext) GetInboundTag() string { return c.inbound }

func (c fakeContext) GetTargetDomain() string { return c.domain }

func (c fakeContext) GetTargetPort() xnet.Port { return c.port }

func (c fakeContext) GetNetwork() xnet.Network { return c.network }

func (c fakeContext) GetProtocol() string { return c.protocol }

func (c fakeContext) GetTargetIPs() []xnet.IP {
	if c.ip == nil {
		return nil
	}
	return []xnet.IP{c.ip}
}

// The policy becomes Xray's router: the instance runs it for every
// connection, sniffed names and protocols included.
func TestPolicyRunsAsXrayRouter(t *testing.T) {
	rt, err := route.CompileRoute(`[{"protocol": "quic", "action": "reject"}, {"domain_suffix": "example.org", "port": 443, "outbound": "direct"}]`, nil)
	if err != nil {
		t.Fatal(err)
	}
	policy, err := route.NewPolicy(xrayconf.RoutingRule, rt)
	if err != nil {
		t.Fatal(err)
	}
	config, err := xrayconf.Build(xrayconf.Document(xrayconf.Obj{"tag": "proxy", "protocol": "freedom"}, testListen(xrayconf.ModeProxy), xrayconf.RoutingRule, "warning"))
	if err != nil {
		t.Fatal(err)
	}
	inst, err := core.NewWithContext(WithPolicy(context.Background(), config, policy), config)
	if err != nil {
		t.Fatal(err)
	}
	defer inst.Close()
	router, ok := inst.GetFeature(routing.RouterType()).(*xrayRouter)
	if !ok {
		t.Fatalf("router is %T", inst.GetFeature(routing.RouterType()))
	}
	for _, c := range []struct {
		ctx  fakeContext
		want string
	}{
		{fakeContext{domain: "WWW.Example.org", port: 443, network: xnet.Network_TCP, protocol: "tls"}, "direct"},
		{fakeContext{domain: "www.example.org", port: 443, network: xnet.Network_UDP, protocol: "quic"}, "block"},
		{fakeContext{domain: "www.baidu.com", port: 80, network: xnet.Network_TCP, protocol: "http1"}, "direct"},
		// The tproxy inbound: a dialled address with the name sniffed from TLS.
		{fakeContext{inbound: xrayconf.TProxyTag, domain: "www.example.org", ip: xnet.ParseIP("93.184.216.34"), port: 443, network: xnet.Network_TCP, protocol: "tls"}, "direct"},
		{fakeContext{ip: xnet.ParseIP("10.0.0.2"), port: 22, network: xnet.Network_TCP}, "direct"},
		{fakeContext{ip: xnet.ParseIP("1.1.1.1"), port: 853, network: xnet.Network_TCP}, "proxy"},
		// The core's DoH servers leave by their tag, whatever the rules say.
		{fakeContext{inbound: xrayconf.DNSRemoteTag, ip: xnet.ParseIP("1.1.1.1"), port: 443, network: xnet.Network_TCP}, "proxy"},
		{fakeContext{inbound: xrayconf.DNSDirectTag, ip: xnet.ParseIP("223.5.5.5"), port: 443, network: xnet.Network_TCP}, "direct"},
		// DNS the gateway intercepts is answered by the core, IPv4 or IPv6,
		// TCP or UDP; through the mixed port it is routed.
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("1.1.1.1"), port: 53, network: xnet.Network_UDP}, xrayconf.DNSOutTag},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("10.10.10.10"), port: 53, network: xnet.Network_UDP}, xrayconf.DNSOutTag},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("2606:4700:4700::1111"), port: 53, network: xnet.Network_UDP}, xrayconf.DNSOutTag},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("8.8.8.8"), port: 53, network: xnet.Network_TCP}, xrayconf.DNSOutTag},
		{fakeContext{inbound: "mixed", ip: xnet.ParseIP("192.168.1.1"), port: 53, network: xnet.Network_UDP}, "direct"},
		// Other tproxy traffic follows the rules; IPv6 is not blocked (that was TUN's).
		{fakeContext{inbound: xrayconf.TProxyTag, domain: "www.example.com", port: 443, network: xnet.Network_UDP, protocol: "quic"}, "block"},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("2606:2800:220:1:248:1893:25c8:1946"), port: 443, network: xnet.Network_TCP}, "proxy"},
		{fakeContext{inbound: xrayconf.TProxyTag, ip: xnet.ParseIP("::ffff:1.1.1.1"), port: 853, network: xnet.Network_TCP}, "proxy"},
		{fakeContext{inbound: "mixed", ip: xnet.ParseIP("2606:2800:220:1:248:1893:25c8:1946"), port: 443, network: xnet.Network_TCP}, "proxy"},
	} {
		decision, err := router.PickRoute(c.ctx)
		if err != nil || decision.GetOutboundTag() != c.want {
			t.Errorf("%+v: got %v %v, want %s", c.ctx, decision.GetOutboundTag(), err, c.want)
		}
	}
}

// testListen is the listen of a start in mode with the default ports and mark.
func testListen(mode string) xrayconf.Listen {
	l := xrayconf.Listen{Mode: mode, Port: xrayconf.DefaultPort}
	if mode == xrayconf.ModeGateway {
		l.TProxyPort, l.Mark = xrayconf.DefaultTProxyPort, xrayconf.DefaultMark
	}
	return l
}
