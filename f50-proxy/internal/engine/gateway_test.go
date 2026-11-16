package engine

import (
	"encoding/json"
	"reflect"
	"slices"
	"testing"

	"f50/proxy/internal/route"
	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/app/proxyman"
	"github.com/xtls/xray-core/transport/internet"
)

// testListen is what f50d sends for a mode, defaults spelled out.
func testListen(mode string) xrayconf.Listen {
	l := xrayconf.Listen{Mode: mode, Port: xrayconf.DefaultPort}
	if mode == xrayconf.ModeGateway {
		l.TProxyPort, l.Mark = xrayconf.DefaultTProxyPort, xrayconf.DefaultMark
	}
	return l
}

// gatewayRequest is a gateway start with every kind of outbound: the picked
// node behind a front node with a fragment dialer, and another node a rule
// names (a 前置代理 chain of its own).
func gatewayRequest() StartRequest {
	reality := "security=reality&sni=a.example&fp=chrome&pbk=" + tPBK + "&type=tcp"
	tls := "security=tls&sni=a.example&type=tcp"
	return StartRequest{
		Link: vlessLink(reality, "picked"), Name: "picked", Mode: xrayconf.ModeGateway, Routing: xrayconf.RoutingRule,
		Via:       []string{vlessLink(tls+"&fragment=40-60,30-50,tlshello", "front")},
		RulesJSON: `{"rules": [{"domain_suffix": ["jp.example"], "outbound": "JP"}], "final": "proxy"}`,
		Nodes: []NamedLink{
			{Name: "JP", Link: "hysteria2://pw@hy.example.net:443?sni=hy.example.net#JP"},
			{Name: "unused", Link: vlessLink(tls, "unused")},
		},
	}
}

func gatewayPlan(t *testing.T, req StartRequest) *runPlan {
	t.Helper()
	if err := req.normalize(); err != nil {
		t.Fatal(err)
	}
	p, err := plan(req, "warning")
	if err != nil {
		t.Fatal(err)
	}
	return p
}

func TestStartRequestDefaults(t *testing.T) {
	req := StartRequest{Mode: xrayconf.ModeGateway}
	if err := req.normalize(); err != nil {
		t.Fatal(err)
	}
	if req.Port != 17890 || req.TProxyPort != 17892 || req.Mark != 255 || req.Routing != xrayconf.RoutingGlobal {
		t.Fatalf("defaults %+v", req)
	}
	req = StartRequest{Mode: xrayconf.ModeGateway, Port: 1080, TProxyPort: 7893, Mark: 0x162}
	if err := req.normalize(); err != nil || req.TProxyPort != 7893 || req.Mark != 0x162 {
		t.Fatalf("given values kept: %+v %v", req, err)
	}
	req = StartRequest{Mode: xrayconf.ModeProxy, TProxyPort: 7893, Mark: 9}
	if err := req.normalize(); err != nil || req.TProxyPort != 0 || req.Mark != 0 {
		t.Fatalf("proxy mode has no tproxy or mark: %+v %v", req, err)
	}
	for _, bad := range []StartRequest{
		{Mode: "tun"},
		{Mode: ""},
		{Mode: xrayconf.ModeGateway, TProxyPort: 70000},
		{Mode: xrayconf.ModeGateway, Port: 17892},
		{Mode: xrayconf.ModeGateway, Mark: -1},
		{Mode: xrayconf.ModeProxy, Routing: "smart"},
	} {
		if err := bad.normalize(); err == nil {
			t.Errorf("%+v: accepted", bad)
		}
	}
}

func TestGatewayInbounds(t *testing.T) {
	doc := gatewayPlan(t, gatewayRequest()).doc
	inbounds := doc["inbounds"].([]xrayconf.Obj)
	if len(inbounds) != 2 {
		t.Fatalf("inbounds %v", inbounds)
	}
	mixed, tproxy := inbounds[0], inbounds[1]
	if mixed["tag"] != "mixed" || mixed["listen"] != "127.0.0.1" || mixed["port"] != 17890 || mixed["protocol"] != "socks" {
		t.Errorf("mixed %v", mixed)
	}
	want := xrayconf.Obj{
		"tag": xrayconf.TProxyTag, "listen": "::", "port": 17892, "protocol": "dokodemo-door",
		"settings":       xrayconf.Obj{"network": "tcp,udp", "followRedirect": true},
		"streamSettings": xrayconf.Obj{"sockopt": xrayconf.Obj{"tproxy": "tproxy", "mark": 255}},
		"sniffing":       xrayconf.Obj{"enabled": true, "destOverride": []string{"http", "tls", "quic"}, "routeOnly": false},
	}
	if !reflect.DeepEqual(tproxy, want) {
		got, _ := json.Marshal(tproxy)
		t.Errorf("tproxy inbound %s", got)
	}

	// Xray reads the fields as meant: a transparent socket, the original
	// destination, the mark.
	config, err := xrayconf.Build(doc)
	if err != nil {
		t.Fatal(err)
	}
	in := config.Inbound[1]
	raw, err := in.ReceiverSettings.GetInstance()
	if err != nil {
		t.Fatal(err)
	}
	rc := raw.(*proxyman.ReceiverConfig)
	so := rc.StreamSettings.GetSocketSettings()
	if in.Tag != xrayconf.TProxyTag || so.GetTproxy() != internet.SocketConfig_TProxy || so.GetMark() != 255 || !rc.ReceiveOriginalDestination {
		t.Errorf("built tproxy inbound: tag %s, tproxy %v, mark %d, follow %v", in.Tag, so.GetTproxy(), so.GetMark(), rc.ReceiveOriginalDestination)
	}
	if rc.SniffingSettings == nil || !rc.SniffingSettings.Enabled || rc.SniffingSettings.RouteOnly {
		t.Errorf("built sniffing %v", rc.SniffingSettings)
	}
}

// Every outbound that opens a socket carries the mark, so nftables never
// catches the core's own traffic: the node, its front node and fragment
// dialer, the other node a rule uses, direct and dns-out.
func TestGatewayMarksEveryOutbound(t *testing.T) {
	p := gatewayPlan(t, gatewayRequest())
	var tags []string
	for _, ob := range p.doc["outbounds"].([]xrayconf.Obj) {
		tag := ob["tag"].(string)
		tags = append(tags, tag)
		mark := at(ob, "streamSettings", "sockopt", "mark")
		if ob["protocol"] == "blackhole" {
			if mark != nil {
				t.Errorf("%s: blackhole marked", tag)
			}
			continue
		}
		if mark != 255 {
			t.Errorf("%s: mark %v", tag, mark)
		}
	}
	for _, want := range []string{"proxy", "proxy>via1", "proxy>via1>fragment", "direct", "block", xrayconf.DNSOutTag, route.NodeTag("JP")} {
		if !slices.Contains(tags, want) {
			t.Errorf("no outbound %s in %v", want, tags)
		}
	}
	if slices.Contains(tags, route.NodeTag("unused")) {
		t.Errorf("a node no rule names got an outbound: %v", tags)
	}
	config, err := xrayconf.Build(p.doc)
	if err != nil {
		t.Fatal(err)
	}
	for _, ob := range config.Outbound {
		raw, err := ob.SenderSettings.GetInstance()
		if err != nil {
			t.Fatal(err)
		}
		mark := raw.(*proxyman.SenderConfig).GetStreamSettings().GetSocketSettings().GetMark()
		if want := int32(255); ob.Tag == "block" {
			want = 0
			if mark != want {
				t.Errorf("built %s: mark %d", ob.Tag, mark)
			}
		} else if mark != want {
			t.Errorf("built %s: mark %d, want %d", ob.Tag, mark, want)
		}
	}

	// Proxy mode marks nothing.
	req := gatewayRequest()
	req.Mode = xrayconf.ModeProxy
	for _, ob := range gatewayPlan(t, req).doc["outbounds"].([]xrayconf.Obj) {
		if at(ob, "streamSettings", "sockopt", "mark") != nil {
			t.Errorf("proxy mode: %s marked", ob["tag"])
		}
	}
}

// Gateway DNS answers no AAAA and direct dials IPv4 only; the routing's
// split stays as in proxy mode.
func TestGatewayDNS(t *testing.T) {
	doc := gatewayPlan(t, gatewayRequest()).doc
	dns := doc["dns"].(xrayconf.Obj)
	if dns["queryStrategy"] != "UseIPv4" {
		t.Errorf("queryStrategy %v", dns["queryStrategy"])
	}
	var tags []string
	for _, s := range dns["servers"].([]xrayconf.Obj) {
		tags = append(tags, s["tag"].(string))
	}
	if !reflect.DeepEqual(tags, []string{xrayconf.DNSRemoteTag, xrayconf.DNSDirectTag}) {
		t.Errorf("servers %v", tags)
	}
	for _, ob := range doc["outbounds"].([]xrayconf.Obj) {
		if ob["tag"] == "direct" && at(ob, "settings", "domainStrategy") != "ForceIPv4" {
			t.Errorf("direct %v", ob)
		}
	}
	req := gatewayRequest()
	req.Mode = xrayconf.ModeProxy
	proxyDoc := gatewayPlan(t, req).doc
	if q := proxyDoc["dns"].(xrayconf.Obj)["queryStrategy"]; q != "UseIP" {
		t.Errorf("proxy queryStrategy %v", q)
	}
	if len(proxyDoc["inbounds"].([]xrayconf.Obj)) != 1 {
		t.Errorf("proxy mode has only the mixed inbound")
	}
}

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
		m, ok := v.(xrayconf.Obj)
		if !ok {
			return nil
		}
		v = m[k]
	}
	return v
}
