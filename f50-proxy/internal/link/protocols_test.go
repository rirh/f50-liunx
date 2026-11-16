package link

import (
	"encoding/base64"
	"strings"
	"testing"
)

// The protocols beyond VLESS and Hysteria2, in the forms the subgateway and
// the usual clients (v2rayN, Shadowrocket, Clash exports) write them.
func TestOtherProtocols(t *testing.T) {
	vmessJSON := base64.StdEncoding.EncodeToString([]byte(`{"v":"2","ps":"VM WS","add":"v.example","port":443,"id":"` + tUUID + `","aid":"0","scy":"auto","net":"ws","type":"none","host":"v.example","path":"/vm","tls":"tls","sni":"v.example","fp":"chrome"}`))
	vmessGRPC := base64.StdEncoding.EncodeToString([]byte(`{"ps":"VM gRPC","add":"10.0.0.2","port":"8443","id":"` + tUUID + `","aid":"64","net":"grpc","path":"svc","type":"multi","tls":""}`))
	cases := []struct {
		link, name, kind, protocol, summary, problem string
		check                                        func(t *testing.T, ob obj)
	}{
		{link: "ss://YWVzLTI1Ni1nY206cHc@1.2.3.4:8388#SS%20SIP002", name: "SS SIP002", kind: "ss", protocol: "shadowsocks", summary: "SHADOWSOCKS / AES-256-GCM / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "settings", "method") != "aes-256-gcm" || at(ob, "settings", "password") != "pw" {
					t.Errorf("settings %v", ob["settings"])
				}
			}},
		{link: "ss://2022-blake3-aes-128-gcm:" + "AAAAAAAAAAAAAAAAAAAAAA%3D%3D" + "@s.example:443#SS%202022", name: "SS 2022", kind: "ss", protocol: "shadowsocks", summary: "SHADOWSOCKS / 2022-BLAKE3-AES-128-GCM / UDP"},
		{link: "ss://" + b64("chacha20-ietf-poly1305:p:w@1.2.3.4:8388") + "#SS%20legacy", name: "SS legacy", kind: "ss", protocol: "shadowsocks", summary: "SHADOWSOCKS / CHACHA20-IETF-POLY1305 / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "settings", "password") != "p:w" {
					t.Errorf("password %v", at(ob, "settings", "password"))
				}
			}},
		{link: "ss://YWVzLTI1Ni1nY206cHc@1.2.3.4:8388/?plugin=obfs-local%3Bobfs%3Dhttp#SS%20plugin", name: "SS plugin", problem: "Shadowsocks 插件 obfs-local 不支持"},
		{link: "ss://rc4-md5:pw@1.2.3.4:8388#SS%20rc4", name: "SS rc4", problem: "infra/conf: failed to build outbound config"},
		{link: "socks5://u:p%40ss@1.2.3.4:1080#S5", name: "S5", kind: "socks", protocol: "socks", summary: "SOCKS5 / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "settings", "user") != "u" || at(ob, "settings", "pass") != "p@ss" {
					t.Errorf("settings %v", ob["settings"])
				}
			}},
		{link: "socks://" + b64("u:p") + "@1.2.3.4:1080#S5%20b64", name: "S5 b64", kind: "socks", protocol: "socks", summary: "SOCKS5 / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "settings", "pass") != "p" {
					t.Errorf("settings %v", ob["settings"])
				}
			}},
		{link: "socks5://1.2.3.4:1080#S5%20open", name: "S5 open", kind: "socks", protocol: "socks", summary: "SOCKS5 / UDP"},
		{link: "http://u:p@1.2.3.4:3128#HTTP", name: "HTTP", kind: "http", protocol: "http", summary: "HTTP"},
		{link: "https://u:p@h.example:443#HTTPS", name: "HTTPS", kind: "http", protocol: "http", summary: "HTTP / TLS",
			check: func(t *testing.T, ob obj) {
				if at(ob, "streamSettings", "tlsSettings", "serverName") != "h.example" {
					t.Errorf("stream %v", ob["streamSettings"])
				}
			}},
		{link: "https://sub.example:443/api/v1/s/abc", name: "sub.example", problem: "这是网址"},
		{link: "trojan://p%3Aw@t.example:443?sni=t.example&type=ws&path=%2Ftj&host=t.example#TJ%20WS", name: "TJ WS", kind: "ws", protocol: "trojan", summary: "TROJAN / WS / TLS / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "settings", "password") != "p:w" || at(ob, "streamSettings", "wsSettings", "path") != "/tj" {
					t.Errorf("outbound %v", ob)
				}
			}},
		{link: "trojan://pw@t.example:443?security=reality&sni=a.example&pbk=" + tPBK + "&sid=ab#TJ%20Reality", name: "TJ Reality", kind: "reality", protocol: "trojan", summary: "TROJAN / REALITY / UDP"},
		{link: "trojan://pw@t.example:443?allowInsecure=1#TJ%20insecure", name: "TJ insecure", problem: "Xray v26 不再跳过证书校验"},
		{link: "trojan://pw@1.2.3.4:443?security=none#TJ%20plain", name: "TJ plain", problem: "infra/conf: failed to build outbound config"},
		{link: "vmess://" + vmessJSON, name: "VM WS", kind: "ws", protocol: "vmess", summary: "VMESS / WS / TLS / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "settings", "id") != tUUID || at(ob, "streamSettings", "wsSettings", "host") != "v.example" || at(ob, "streamSettings", "tlsSettings", "serverName") != "v.example" {
					t.Errorf("outbound %v", ob)
				}
			}},
		{link: "vmess://" + vmessGRPC, name: "VM gRPC", kind: "grpc", protocol: "vmess", summary: "VMESS / GRPC / UDP",
			check: func(t *testing.T, ob obj) {
				if at(ob, "streamSettings", "grpcSettings", "serviceName") != "svc" || at(ob, "streamSettings", "grpcSettings", "multiMode") != true {
					t.Errorf("stream %v", ob["streamSettings"])
				}
			}},
		{link: "vmess://" + tUUID + "@v.example:443?encryption=chacha20-poly1305&security=tls&type=tcp#VM%20URL", name: "VM URL", kind: "tls", protocol: "vmess", summary: "VMESS / TLS / CHACHA20-POLY1305 / UDP"},
		{link: "vmess://" + tUUID + "@v.example:443?encryption=aes-256-cfb#VM%20cfb", name: "VM cfb", problem: "VMess 加密 aes-256-cfb 不支持"},
		{link: "vmess://" + base64.StdEncoding.EncodeToString([]byte(`{"ps":"VM http","add":"v.example","port":80,"id":"`+tUUID+`","net":"tcp","type":"http"}`)), name: "VM http", problem: "TCP http 伪装不支持"},
	}
	var lines []string
	for _, c := range cases {
		lines = append(lines, c.link)
	}
	nodes := ParseSubscription(base64.StdEncoding.EncodeToString([]byte(strings.Join(lines, "\n"))))
	if len(nodes) != len(cases) {
		t.Fatalf("got %d nodes", len(nodes))
	}
	for i, c := range cases {
		n := nodes[i]
		if n.Name != c.name {
			t.Errorf("%d: name %q, want %q", i, n.Name, c.name)
		}
		if c.problem != "" {
			if !strings.HasPrefix(n.Problem, c.problem) {
				t.Errorf("%s: problem %q, want %q", c.name, n.Problem, c.problem)
			}
			continue
		}
		if n.Problem != "" || n.Kind != c.kind || n.Protocol != c.protocol || n.Summary != c.summary {
			t.Errorf("%s: got kind=%q protocol=%q summary=%q problem=%q", c.name, n.Kind, n.Protocol, n.Summary, n.Problem)
			continue
		}
		// The stored link is already normalized: parsing it again changes nothing.
		if normalizeLink(n.Link) != n.Link {
			t.Errorf("%s: %s normalizes again to %s", c.name, n.Link, normalizeLink(n.Link))
		}
		_, ob, _ := Outbound(n.Link)
		if c.check != nil {
			c.check(t, ob)
		}
		loads(t, n.Link)
	}
}
