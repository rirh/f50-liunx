package link

import (
	"encoding/base64"
	"fmt"
	"strings"
	"testing"

	"f50/proxy/internal/xrayconf"
)

func b64(s string) string { return base64.RawStdEncoding.EncodeToString([]byte(s)) }

// Links in the shapes the One Node subgateway shareurl package renders, then the
// same lines as Shadowrocket exports them. One makes Xray's VLESS builder panic;
// parsing must turn that into a problem.
func TestParseSubgatewayLinks(t *testing.T) {
	sub := "vless://u@1.2.3.4:443?encryption=none&flow=xtls-rprx-vision&fp=chrome&headerType=none&pbk=mZ8T6LkqEdo2V5cXsUu0b7K1nY4wJr3gH9fQaPxCtSe&security=reality&sid=ab&sni=bt.example&type=tcp#LAX%20Reality\n" +
		"vless://u@cdn.example.net:443?encryption=mlkem768x25519plus.native.0rtt.mZ8T6LkqEdo2V5cXsUu0b7K1nY4wJr3gH9fQaPxCtSe&fp=chrome&host=cdn.example.net&mode=packet-up&path=%2Fx&security=tls&sni=cdn.example.net&type=xhttp#LAX%20CDN\n" +
		"hysteria2://pw@hy.example.net:443?mport=40000-49999&sni=hy.example.net#LAX%20Hy2\n" +
		"hysteria2://pw@1.2.3.4:443?insecure=1#self-signed\n" +
		"ss://YWVzLTI1Ni1nY206cHc@1.2.3.4:8388#ss\n" +
		"vless://u@1.2.3.4:443?encryption=mlkem768x25519plus.native.0rtt.short&security=tls&type=xhttp&path=%2Fx#bad-encryption\n" +
		"vless://" + b64("none:u@1.2.3.4:443") + "?remarks=SR%20Reality&tls=1&peer=bt.example&udp=1&xtls=2&pbk=mZ8T6LkqEdo2V5cXsUu0b7K1nY4wJr3gH9fQaPxCtSe&sid=ab&fingerprint=chrome\n" +
		"vless://" + b64("mlkem768x25519plus.native.0rtt.mZ8T6LkqEdo2V5cXsUu0b7K1nY4wJr3gH9fQaPxCtSe:u@cdn.example.net:443") +
		"?path=/x&remarks=SR%20CDN&obfsParam=cdn.example.net&obfs=xhttp&tls=1&peer=cdn.example.net&udp=1&mode=packet-up&fingerprint=chrome\n" +
		"hysteria2://a+b%2Fc=@1.2.3.4:29095?peer=hy.example.net&mport=29058-29157#SR%20Hy2\n"
	nodes := ParseSubscription(base64.StdEncoding.EncodeToString([]byte(sub)))
	want := []struct{ name, kind, problem string }{
		{"LAX Reality", "reality", ""},
		{"LAX CDN", "xhttp", ""},
		{"LAX Hy2", "hy2", ""},
		{"self-signed", "hy2", "hysteria2 insecure=1 needs pinnedPeerCertSha256"},
		{"ss", "ss", ""},
		{"bad-encryption", "xhttp", "xray rejected the config: "},
		{"SR Reality", "reality", ""},
		{"SR CDN", "xhttp", ""},
		{"SR Hy2", "hy2", ""},
	}
	if len(nodes) != len(want) {
		t.Fatalf("got %d nodes", len(nodes))
	}
	for i, w := range want {
		n := nodes[i]
		if n.Name != w.name || n.Kind != w.kind || !strings.HasPrefix(n.Problem, w.problem) || (w.problem == "") != (n.Problem == "") {
			t.Errorf("node %d = %+v, want %+v", i, n, w)
		}
	}
	// The Shadowrocket forms carry the same settings as the standard ones.
	sr := map[string]string{"SR Reality": "LAX Reality", "SR CDN": "LAX CDN"}
	for _, n := range nodes[6:8] {
		_, got, _ := Outbound(n.Link)
		for _, m := range nodes[:2] {
			if m.Name == sr[n.Name] {
				_, want, _ := Outbound(m.Link)
				if fmt.Sprint(got["streamSettings"]) != fmt.Sprint(want["streamSettings"]) {
					t.Errorf("%s stream\n got %v\nwant %v", n.Name, got["streamSettings"], want["streamSettings"])
				}
			}
		}
	}
	_, hy2, _ := Outbound(nodes[8].Link)
	stream := hy2["streamSettings"].(obj)
	if stream["tlsSettings"].(obj)["serverName"] != "hy.example.net" || stream["hysteriaSettings"].(obj)["auth"] != "a+b/c=" {
		t.Errorf("hy2 peer/auth: %v", stream)
	}
	// Every supported line must build into a real Xray config in both modes.
	for _, n := range append(nodes[:3:3], nodes[6:]...) {
		_, ob, err := Outbound(n.Link)
		if err != nil {
			t.Fatal(err)
		}
		for _, mode := range []string{xrayconf.ModeProxy, xrayconf.ModeGateway} {
			if _, err := xrayconf.Build(xrayconf.Document(ob, testListen(mode), xrayconf.RoutingRule, "warning")); err != nil {
				t.Errorf("%s %s: %v", n.Name, mode, err)
			}
		}
	}
}

// What the add-node page produces (VLESS over TCP/XHTTP, Hysteria2), plus the
// protocols One Node does not serve, which must be reported, not run.
func TestParseProtocols(t *testing.T) {
	const uuid = "27a4f0d6-7f1a-4c55-9b0a-0d6f7c1b2e3a"
	const pbk = "mZ8T6LkqEdo2V5cXsUu0b7K1nY4wJr3gH9fQaPxCtSe"
	cases := []struct{ link, name, kind, summary, problem string }{
		{"vless://" + uuid + "@1.2.3.4:443?security=reality&sni=a.example&pbk=" + pbk + "&sid=ab&type=tcp&flow=xtls-rprx-vision#VL Reality", "VL Reality", "reality", "VLESS / REALITY / XTLS-RPRX-VISION / UDP", ""},
		{"vless://" + uuid + "@a.example:443?security=reality&sni=a.example&pbk=" + pbk + "&type=xhttp&path=%2Fx#VL R XHTTP", "VL R XHTTP", "xhttp", "VLESS / REALITY / XHTTP / UDP", ""},
		{"vless://" + uuid + "@a.example:443?security=tls&type=xhttp&path=%2Fx&host=a.example#VL TLS", "VL TLS", "xhttp", "VLESS / XHTTP / TLS / UDP", ""},
		{"vless://" + uuid + "@1.2.3.4:8080?type=tcp#VL plain", "VL plain", "", "", "infra/conf: failed to build outbound config"}, // Xray refuses plain VLESS to public addresses
		{"hy2://pw@h.example:443?sni=h.example&mport=20000-20100#HY", "HY", "hy2", "HYSTERIA2 / PORT HOPPING / UDP", ""},
		{"vless://" + uuid + "@a.example:443?security=tls&type=ws&path=%2Fws#VL WS", "VL WS", "ws", "VLESS / WS / TLS / UDP", ""},
		{"vless://" + uuid + "@a.example:443?security=tls&type=kcp#VL KCP", "VL KCP", "", "", `transport "kcp" not supported`},
		{"trojan://pw@t.example:443#TJ", "TJ", "tls", "TROJAN / TLS / UDP", ""},
		{"vmess://e30=", "vmess", "", "", "missing port"},
		{"tuic://u:p@t.example:443#TUIC", "TUIC", "", "", `scheme "tuic" not supported`},
	}
	var lines []string
	for _, c := range cases {
		lines = append(lines, c.link)
	}
	nodes := ParseSubscription(strings.Join(lines, "\n"))
	if len(nodes) != len(cases) {
		t.Fatalf("got %d nodes", len(nodes))
	}
	for i, c := range cases {
		n := nodes[i]
		if c.problem != "" {
			if !strings.HasPrefix(n.Problem, c.problem) {
				t.Errorf("%s: problem %q, want %q", c.name, n.Problem, c.problem)
			}
			continue
		}
		if n.Problem != "" || n.Name != c.name || n.Kind != c.kind || n.Summary != c.summary {
			t.Errorf("%s: got name=%q kind=%q summary=%q problem=%q", c.name, n.Name, n.Kind, n.Summary, n.Problem)
			continue
		}
		_, ob, _ := Outbound(n.Link)
		for _, mode := range []string{xrayconf.ModeProxy, xrayconf.ModeGateway} {
			if _, err := xrayconf.Build(xrayconf.Document(ob, testListen(mode), xrayconf.RoutingRule, "warning")); err != nil {
				t.Errorf("%s %s: %v", c.name, mode, err)
			}
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
