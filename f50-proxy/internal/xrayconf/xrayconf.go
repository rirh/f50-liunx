// Package xrayconf builds the Xray JSON document the engine runs (inbounds, DNS,
// socket marks) and turns it into Xray's protobuf config. It also names the
// tags and attributes the other packages agree on inside Xray.
package xrayconf

import (
	"bytes"
	"encoding/json"
	"fmt"

	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/infra/conf/serial"

	// Every Xray feature a share link or the engine may use; the builders and
	// the instances look them up by type, so whoever builds a config needs them.
	_ "github.com/xtls/xray-core/main/distro/all"
)

// Obj is a JSON object of the Xray document.
type Obj = map[string]any

// Sockopt is an outbound's streamSettings.sockopt, created when missing.
func Sockopt(ob Obj) Obj {
	stream, ok := ob["streamSettings"].(Obj)
	if !ok {
		stream = Obj{}
		ob["streamSettings"] = stream
	}
	so, ok := stream["sockopt"].(Obj)
	if !ok {
		so = Obj{}
		stream["sockopt"] = so
	}
	return so
}

// Modes f50d can ask for. Both keep the loopback mixed port; gateway adds
// the transparent-proxy inbound the nftables rules send the LAN (and, with
// intercept_router, the router itself) to. It needs CAP_NET_ADMIN, which the
// procd service has (IP_TRANSPARENT, SO_MARK).
const (
	ModeProxy   = "proxy"
	ModeGateway = "gateway"
)

// Defaults of the start request's ports and socket mark (DESIGN.md §1).
const (
	DefaultPort       = 17890
	DefaultTProxyPort = 17892
	DefaultMark       = 255
)

// Routing modes: where traffic that reached the core goes. They combine freely
// with ModeProxy/ModeGateway, which decide what traffic reaches the core.
const (
	RoutingGlobal = "global" // everything through the node
	RoutingRule   = "rule"   // the user's rules, then mainland China direct, the rest to the route's final
	RoutingDirect = "direct" // nothing through the node
)

// ProbeInbound marks a test connection: the router sends it to the outbound
// named after the prefix, and the tracker leaves it off the 连接 page.
const ProbeInbound = "probe>"

// GroupAttr names, on a connection's content, the 策略组 that routed it, so
// the tracker can close the group's connections when its node changes.
const GroupAttr = "onenode-group"

// ProbeDocument is an instance with no inbounds, used only through core.Dial:
// a node's outbounds (NodeOutbounds), the first being the default.
func ProbeDocument(outbounds ...Obj) Obj {
	return Obj{
		"log":       Obj{"loglevel": "none"},
		"outbounds": outbounds,
	}
}

// Build turns the JSON document into Xray's protobuf config. Some Xray
// builders panic on malformed input (e.g. a bad VLESS encryption string), and one
// bad subscription line must not take the core down.
func Build(doc Obj) (config *core.Config, err error) {
	defer func() {
		if r := recover(); r != nil {
			config, err = nil, fmt.Errorf("xray rejected the config: %v", r)
		}
	}()
	raw, err := json.Marshal(doc)
	if err != nil {
		return nil, err
	}
	c, err := serial.DecodeJSONConfig(bytes.NewReader(raw))
	if err != nil {
		return nil, err
	}
	return c.Build()
}
