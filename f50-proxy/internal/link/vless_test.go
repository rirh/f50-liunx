package link

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	"f50/proxy/internal/logtap"
	"f50/proxy/internal/route"
	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/core"
)

const (
	tUUID = "27a4f0d6-7f1a-4c55-9b0a-0d6f7c1b2e3a"
	tPBK  = "mZ8T6LkqEdo2V5cXsUu0b7K1nY4wJr3gH9fQaPxCtSe"
)

func vlessLink(query, name string) string {
	return "vless://" + tUUID + "@a.example:443?" + query + "#" + name
}

// at walks nested objects: at(ob, "streamSettings", "wsSettings", "path").
func at(v any, keys ...string) any {
	for _, k := range keys {
		m, ok := v.(obj)
		if !ok {
			return nil
		}
		v = m[k]
	}
	return v
}

// loads checks Xray takes the node's outbounds, as the picked node and as a
// group's node, in both modes.
func loads(t *testing.T, link string) {
	t.Helper()
	for _, mode := range []string{xrayconf.ModeProxy, xrayconf.ModeGateway} {
		_, proxy, err := NodeOutbounds(link, "proxy")
		if err != nil {
			t.Fatal(err)
		}
		_, other, _ := NodeOutbounds(link, route.NodeTag("other"))
		doc := xrayconf.Document(proxy[0], testListen(mode), xrayconf.RoutingRule, "warning")
		doc["outbounds"] = append(append(doc["outbounds"].([]obj), proxy[1:]...), other...)
		config, err := xrayconf.Build(doc)
		if err != nil {
			t.Fatalf("%s %s: build: %v", link, mode, err)
		}
		if mode == xrayconf.ModeProxy { // the tproxy inbound needs root to start, not to build
			inst, err := core.New(config)
			logtap.Reinstall()
			if err != nil {
				t.Fatalf("%s: core.New: %v", link, err)
			}
			_ = inst.Close()
		}
	}
}

func TestVLESSTransports(t *testing.T) {
	for _, c := range []struct {
		query, kind, summary string
		path                 []string
		want                 any
	}{
		{"security=tls&type=ws&path=%2Fws&host=cdn.example", "ws", "VLESS / WS / TLS / UDP", []string{"wsSettings"}, obj{"path": "/ws", "host": "cdn.example"}},
		{"security=tls&type=ws", "ws", "VLESS / WS / TLS / UDP", []string{"wsSettings"}, obj{"path": "/"}},
		{"security=tls&type=httpupgrade&path=%2Fup&host=cdn.example", "httpupgrade", "VLESS / HTTPUPGRADE / TLS / UDP", []string{"httpupgradeSettings"}, obj{"path": "/up", "host": "cdn.example"}},
		{"security=tls&type=grpc&serviceName=svc", "grpc", "VLESS / GRPC / TLS / UDP", []string{"grpcSettings"}, obj{"serviceName": "svc"}},
		{"security=tls&type=grpc&serviceName=svc&mode=gun", "grpc", "VLESS / GRPC / TLS / UDP", []string{"grpcSettings"}, obj{"serviceName": "svc"}},
		{"security=tls&type=grpc&serviceName=svc&mode=multi&authority=a.example", "grpc", "VLESS / GRPC / TLS / UDP", []string{"grpcSettings"}, obj{"serviceName": "svc", "multiMode": true, "authority": "a.example"}},
		{"security=reality&sni=a.example&pbk=" + tPBK + "&sid=ab&type=grpc&serviceName=svc", "grpc", "VLESS / REALITY / GRPC / UDP", []string{"realitySettings", "fingerprint"}, "chrome"},
		{"security=tls&type=raw", "tls", "VLESS / TLS / UDP", []string{"network"}, "tcp"},
		{"security=tls&type=splithttp&path=%2Fx", "xhttp", "VLESS / XHTTP / TLS / UDP", []string{"xhttpSettings", "mode"}, "auto"},
	} {
		link := vlessLink(c.query, "n")
		p, err := Parse(link)
		if err != nil {
			t.Errorf("%s: %v", c.query, err)
			continue
		}
		got := at(p.Outbound["streamSettings"], c.path...)
		if p.Kind != c.kind || p.Summary != c.summary || !reflect.DeepEqual(got, c.want) {
			t.Errorf("%s: kind %q summary %q %v = %#v", c.query, p.Kind, p.Summary, c.path, got)
		}
		loads(t, link)
	}
}

func TestVLESSSecurity(t *testing.T) {
	p, err := Parse(vlessLink("security=tls&type=ws&sni=s.example&alpn=h2,http%2F1.1&fp=Firefox&pcs=ab12,cd34&vcn=cdn.example&ech=AEX%2B", "n"))
	if err != nil {
		t.Fatal(err)
	}
	want := obj{
		"serverName": "s.example", "fingerprint": "firefox", "alpn": []string{"h2", "http/1.1"},
		"pinnedPeerCertSha256": "ab12,cd34", "verifyPeerCertByName": "cdn.example", "echConfigList": "AEX+",
	}
	if got := at(p.Outbound, "streamSettings", "tlsSettings"); !reflect.DeepEqual(got, want) {
		t.Fatalf("tls %#v", got)
	}
	// TLS defaults: sni from host, then the server; chrome.
	p, _ = Parse(vlessLink("security=tls&type=ws&host=cdn.example", "n"))
	if got := at(p.Outbound, "streamSettings", "tlsSettings"); !reflect.DeepEqual(got, obj{"serverName": "cdn.example", "fingerprint": "chrome"}) {
		t.Fatalf("tls defaults %#v", got)
	}
	for _, fp := range fingerprints {
		if _, err := Parse(vlessLink("security=reality&pbk="+tPBK+"&sni=a.example&fp="+fp, "n")); err != nil {
			t.Errorf("fp %s: %v", fp, err)
		}
	}
	for query, want := range map[string]string{
		"security=tls&fp=hellochrome_120":                    "指纹 fp=hellochrome_120 不支持",
		"security=reality&pbk=" + tPBK + "&fp=unsafe":        "指纹 fp=unsafe 不支持",
		"security=tls&allowInsecure=1":                       "改用 pcs 固定证书指纹",
		"security=tls&insecure=1":                            "改用 pcs 固定证书指纹",
		"security=xtls":                                      `security "xtls" not supported`,
		"security=tls&type=grpc&mode=stream":                 `grpc mode "stream" 无效`,
		"security=reality&pbk=" + tPBK + "&type=ws":          "REALITY 只支持 tcp、xhttp、grpc 传输，不支持 ws",
		"security=reality&pbk=" + tPBK + "&type=httpupgrade": "不支持 httpupgrade",
		"security=tls&type=h2":                               `transport "h2" not supported`,
		"security=tls&type=quic":                             `transport "quic" not supported`,
		"security=tls&type=xhttp&extra=%7Bx":                 "xhttp extra is not JSON",
	} {
		if _, err := Parse(vlessLink(query, "n")); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%s: %v, want %q", query, err, want)
		}
	}
	loads(t, vlessLink("security=tls&type=ws&sni=s.example&alpn=h2,http%2F1.1&fp=safari&vcn=cdn.example", "n"))
}

// flow only means something on TCP with TLS or REALITY; elsewhere it is dropped with a warning.
func TestVLESSFlow(t *testing.T) {
	for query, kept := range map[string]bool{
		"security=reality&pbk=" + tPBK + "&sni=a.example&type=tcp&flow=xtls-rprx-vision": true,
		"security=tls&flow=xtls-rprx-vision":                                             true,
		"security=tls&type=ws&flow=xtls-rprx-vision":                                     false,
		"security=reality&pbk=" + tPBK + "&type=xhttp&flow=xtls-rprx-vision":             false,
	} {
		p, err := Parse(vlessLink(query, "n"))
		if err != nil {
			t.Fatalf("%s: %v", query, err)
		}
		flow := at(p.Outbound, "settings", "flow")
		if kept != (flow == "xtls-rprx-vision") || kept != (len(p.Warnings) == 0) || kept != strings.Contains(p.Summary, "VISION") {
			t.Errorf("%s: flow %v warnings %v summary %q", query, flow, p.Warnings, p.Summary)
		}
	}
	// The listing shows the warning.
	nodes := ParseSubscription(vlessLink("security=tls&type=ws&flow=xtls-rprx-vision", "ws-flow"))
	if nodes[0].Problem != "" || len(nodes[0].Warnings) != 1 || !strings.Contains(nodes[0].Warnings[0], "已忽略") {
		t.Fatalf("node %+v", nodes[0])
	}
	// Plain VLESS (security none) to a loopback test server keeps no flow either.
	p, _ := Parse("vless://" + tUUID + "@127.0.0.1:8080?type=tcp&flow=xtls-rprx-vision#x")
	if at(p.Outbound, "settings", "flow") != nil || len(p.Warnings) != 1 {
		t.Fatalf("security none: %+v", p)
	}
}

func TestVLESSExtensions(t *testing.T) {
	tls := "security=tls&type=ws&path=%2Fws"
	reality := "security=reality&pbk=" + tPBK + "&sni=a.example&sid=ab&type=tcp"

	p, err := Parse(vlessLink(tls+"&mux=8&tfo=1", "n"))
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(p.Outbound["mux"], obj{"enabled": true, "concurrency": 8}) || at(p.Outbound, "streamSettings", "sockopt", "tcpFastOpen") != true {
		t.Fatalf("mux/tfo %#v", p.Outbound)
	}
	if p.Summary != "VLESS / WS / TLS / UDP / MUX" || p.Dialer != nil {
		t.Fatalf("summary %q dialer %v", p.Summary, p.Dialer)
	}
	loads(t, vlessLink(tls+"&mux=8&tfo=1", "n"))
	for query, want := range map[string]string{
		reality + "&flow=xtls-rprx-vision&mux=8":  "mux 不能和 flow xtls-rprx-vision 同时使用",
		tls + "&mux=0x10":                         "mux=0x10 无效",
		tls + "&mux=1025":                         "mux=1025 无效",
		tls + "&fragment=40-60,30-50":             "需要 <长度>,<间隔>,<packets>",
		tls + "&fragment=60-40,30-50,tlshello":    "fragment=60-40,30-50,tlshello 无效",
		tls + "&fragment=40-60,30-50,clienthello": "无效",
		tls + "&fragment=a,30-50,tlshello":        "无效",
	} {
		if _, err := Parse(vlessLink(query, "n")); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%s: %v, want %q", query, err, want)
		}
	}
	if _, err := Parse(vlessLink(tls+"&mux=0", "n")); err != nil {
		t.Errorf("mux=0 means off: %v", err)
	}

	// fragment: a freedom dialer per node, named after the node's tag; tfo moves to it.
	link := vlessLink(reality+"&flow=xtls-rprx-vision&fragment=40-60,30-50,tlshello&tfo=1", "frag")
	for _, tag := range []string{"proxy", route.NodeTag("HK-1")} {
		kind, obs, err := NodeOutbounds(link, tag)
		if err != nil || kind != "reality" || len(obs) != 2 {
			t.Fatalf("%s: %s %v %v", tag, kind, obs, err)
		}
		node, dialer := obs[0], obs[1]
		if node["tag"] != tag || dialer["tag"] != tag+">fragment" || at(node, "streamSettings", "sockopt", "dialerProxy") != tag+">fragment" {
			t.Fatalf("%s: tags %v / %v", tag, node, dialer)
		}
		wantDialer := obj{"tag": tag + ">fragment", "protocol": "freedom",
			"settings":       obj{"fragment": obj{"packets": "tlshello", "length": "40-60", "interval": "30-50"}},
			"streamSettings": obj{"sockopt": obj{"tcpFastOpen": true}}}
		if !reflect.DeepEqual(dialer, wantDialer) {
			t.Fatalf("dialer %#v", dialer)
		}
		if at(node, "streamSettings", "sockopt", "tcpFastOpen") != nil {
			t.Fatalf("tfo left on the node: %v", node)
		}
	}
	if p, _ := Parse(link); p.Summary != "VLESS / REALITY / XTLS-RPRX-VISION / UDP / FRAGMENT" {
		t.Fatalf("summary %q", p.Summary)
	}
	loads(t, link)
	loads(t, vlessLink(tls+"&fragment=1-3,5,1-3", "n"))
	// Without TLS there is no ClientHello to split.
	p, err = Parse("vless://" + tUUID + "@127.0.0.1:8080?type=tcp&fragment=40-60,30-50,tlshello#x")
	if err != nil || p.Dialer != nil || len(p.Warnings) != 1 {
		t.Fatalf("fragment without TLS: %+v %v", p, err)
	}

	// The interface binding goes on whatever opens the socket.
	BindInterface = "en9"
	defer func() { BindInterface = "" }()
	_, obs, _ := NodeOutbounds(link, "proxy")
	if at(obs[1], "streamSettings", "sockopt", "interface") != "en9" || at(obs[0], "streamSettings", "sockopt", "interface") != nil {
		t.Fatalf("interface on %v", obs)
	}
	_, obs, _ = NodeOutbounds(vlessLink(tls+"&tfo=1", "n"), "proxy")
	if so := at(obs[0], "streamSettings", "sockopt"); !reflect.DeepEqual(so, obj{"tcpFastOpen": true, "interface": "en9"}) {
		t.Fatalf("sockopt %v", so)
	}
}

// Shadowrocket's export of ws and grpc nodes reads as the standard form.
func TestShadowrocketTransports(t *testing.T) {
	for raw, want := range map[string]obj{
		"vless://" + b64("auto:"+tUUID+"@a.example:443") + "?remarks=SR%20WS&tls=1&peer=a.example&obfs=websocket&obfsParam=cdn.example&path=/ws": {"network": "ws", "wsSettings": obj{"path": "/ws", "host": "cdn.example"}},
		"vless://" + b64("auto:"+tUUID+"@a.example:443") + "?remarks=SR%20gRPC&tls=1&peer=a.example&obfs=grpc&path=svc":                          {"network": "grpc", "grpcSettings": obj{"serviceName": "svc"}},
	} {
		p, err := Parse(raw)
		if err != nil {
			t.Fatalf("%s: %v", raw, err)
		}
		stream := p.Outbound["streamSettings"].(obj)
		for k, v := range want {
			if !reflect.DeepEqual(stream[k], v) {
				b, _ := json.Marshal(stream)
				t.Errorf("%s: %s = %v in %s", p.Name, k, stream[k], b)
			}
		}
		loads(t, raw)
	}
}

// 前置代理: each hop dials through the next; only the last opens the socket,
// so it alone keeps its fragment dialer and the interface binding.
func TestChainOutbounds(t *testing.T) {
	reality := "security=reality&sni=a.example&fp=chrome&pbk=" + tPBK + "&type=tcp"
	tls := "security=tls&sni=a.example&type=tcp"
	node := vlessLink(reality+"&fragment=40-60,30-50,tlshello", "node")
	front := vlessLink(tls+"&tfo=1", "front")
	last := vlessLink(tls+"&fragment=40-60,30-50,tlshello", "last")
	BindInterface = "en9"
	defer func() { BindInterface = "" }()

	tag := route.NodeTag("HK-1")
	kind, obs, err := ChainOutbounds(node, []string{front, last}, tag)
	if err != nil || kind != "reality" || len(obs) != 4 {
		t.Fatalf("%s %v %v", kind, obs, err)
	}
	want := [][2]any{
		{tag, tag + ">via1"},
		{tag + ">via1", tag + ">via2"},
		{tag + ">via2", tag + ">via2>fragment"},
		{tag + ">via2>fragment", nil},
	}
	for i, ob := range obs {
		if ob["tag"] != want[i][0] || at(ob, "streamSettings", "sockopt", "dialerProxy") != want[i][1] {
			t.Fatalf("hop %d: %v", i, ob)
		}
		if bound := at(ob, "streamSettings", "sockopt", "interface"); (bound != nil) != (i == 3) {
			t.Fatalf("interface on hop %d: %v", i, ob)
		}
	}
	doc := xrayconf.ProbeDocument(obs...)
	if _, err := xrayconf.Build(doc); err != nil {
		t.Fatal(err)
	}

	// No front nodes: the same as NodeOutbounds.
	_, plain, _ := NodeOutbounds(node, "proxy")
	_, chained, _ := ChainOutbounds(node, nil, "proxy")
	if !reflect.DeepEqual(plain, chained) {
		t.Fatalf("%v != %v", plain, chained)
	}
	if _, _, err := ChainOutbounds(node, []string{"nope://x"}, "proxy"); err == nil || !strings.Contains(err.Error(), "前置代理 1") {
		t.Fatalf("bad front link: %v", err)
	}
}
