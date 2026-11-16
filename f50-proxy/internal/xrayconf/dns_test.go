package xrayconf

import (
	"slices"
	"strings"
	"testing"

	"f50/proxy/internal/geodata"
)

// Every routing mode builds with its DNS: 规则 splits mainland names to the
// direct server, 全局 and 直连 use one server, and nothing falls back to the
// system resolver.
func TestDocumentDNS(t *testing.T) {
	want := map[string][]string{
		RoutingRule:   {DNSRemoteTag, DNSDirectTag},
		RoutingGlobal: {DNSRemoteTag},
		RoutingDirect: {DNSDirectTag},
	}
	for routing, tags := range want {
		for _, mode := range []string{ModeProxy, ModeGateway} {
			doc := Document(Obj{"tag": "proxy", "protocol": "freedom"}, testListen(mode), routing, "warning")
			if _, err := Build(doc); err != nil {
				t.Fatalf("%s %s: %v", routing, mode, err)
			}
			var got []string
			for _, server := range doc["dns"].(Obj)["servers"].([]Obj) {
				got = append(got, server["tag"].(string))
				if routing == RoutingRule && server["tag"] == DNSDirectTag && len(server["domains"].([]string)) < 1000 {
					t.Errorf("%s %s: mainland server has no domain list", routing, mode)
				}
			}
			if strings.Join(got, ",") != strings.Join(tags, ",") {
				t.Errorf("%s %s: servers %v, want %v", routing, mode, got, tags)
			}
		}
	}
}

// The client's own hosts resolve through the direct server in every routing
// that uses the node, and the shared mainland list is never written to.
func TestAppHostsDNS(t *testing.T) {
	before := len(geodata.CNDomains())
	for _, routing := range []string{RoutingRule, RoutingGlobal} {
		doc := Document(Obj{"tag": "proxy", "protocol": "freedom"}, testListen(ModeGateway), routing, "warning")
		AppHostsDNS(doc, routing, []string{"marseo.eu.org"})
		if _, err := Build(doc); err != nil {
			t.Fatalf("%s: %v", routing, err)
		}
		found := false
		for _, server := range doc["dns"].(Obj)["servers"].([]Obj) {
			if server["tag"] == DNSDirectTag {
				found = slices.Contains(server["domains"].([]string), "domain:marseo.eu.org")
			}
		}
		if !found {
			t.Errorf("%s: marseo.eu.org not on the direct DNS server", routing)
		}
	}
	if len(geodata.CNDomains()) != before {
		t.Errorf("mainland list changed: %d → %d", before, len(geodata.CNDomains()))
	}
}

// testListen is the listen of a start in mode with the default ports and mark.
func testListen(mode string) Listen {
	l := Listen{Mode: mode, Port: DefaultPort}
	if mode == ModeGateway {
		l.TProxyPort, l.Mark = DefaultTProxyPort, DefaultMark
	}
	return l
}
