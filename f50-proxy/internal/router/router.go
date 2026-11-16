// Package router replaces Xray's router feature with a route.Policy, so rules
// keep sing-box's semantics instead of being translated into Xray rules.
package router

import (
	"context"
	"errors"
	"net/netip"
	"strings"

	"f50/proxy/internal/route"
	"f50/proxy/internal/rulespec"
	"f50/proxy/internal/rulespec/engine"
	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/common"
	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/common/serial"
	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/features/dns"
	"github.com/xtls/xray-core/features/routing"
	routingdns "github.com/xtls/xray-core/features/routing/dns"
	routingsession "github.com/xtls/xray-core/features/routing/session"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/dynamicpb"
)

// Xray asks its router feature where each connection goes. The core replaces
// Xray's router with One Node's rule engine, so rules keep sing-box's
// semantics instead of being translated into Xray rules. The feature's config
// is a message type described at runtime (no generated code); the policy it
// runs travels in the instance's context.
var routerType = func() protoreflect.MessageType {
	file, err := protodesc.NewFile(&descriptorpb.FileDescriptorProto{
		Name:        proto.String("f50/proxy/router.proto"),
		Package:     proto.String("f50.proxy"),
		Syntax:      proto.String("proto3"),
		MessageType: []*descriptorpb.DescriptorProto{{Name: proto.String("Router")}},
	}, protoregistry.GlobalFiles)
	common.Must(err)
	common.Must(protoregistry.GlobalFiles.RegisterFile(file))
	messageType := dynamicpb.NewMessageType(file.Messages().ByName("Router"))
	common.Must(protoregistry.GlobalTypes.RegisterMessage(messageType))
	common.Must(common.RegisterConfig((*dynamicpb.Message)(nil), func(ctx context.Context, _ interface{}) (interface{}, error) {
		policy, _ := ctx.Value(policyKey{}).(*route.Policy)
		if policy == nil {
			return nil, errors.New("the core started Xray without a routing policy")
		}
		router := &xrayRouter{policy: policy}
		return router, core.RequireFeatures(ctx, func(client dns.Client) error {
			router.dns = client
			return nil
		})
	}))
	return messageType
}()

type policyKey struct{}

// WithPolicy adds the router to a built config and the policy to the
// context the instance is created with.
func WithPolicy(ctx context.Context, config *core.Config, policy *route.Policy) context.Context {
	config.App = append(config.App, serial.ToTypedMessage(routerType.New().Interface()))
	return context.WithValue(ctx, policyKey{}, policy)
}

type xrayRouter struct {
	policy *route.Policy
	dns    dns.Client
}

func (r *xrayRouter) Type() interface{} { return routing.RouterType() }

func (r *xrayRouter) Start() error { return nil }

func (r *xrayRouter) Close() error { return nil }

func (r *xrayRouter) PickRoute(ctx routing.Context) (routing.Route, error) {
	if tag := fixedRoute(ctx); tag != "" {
		return &decision{Context: ctx, tag: tag}, nil
	}
	conn := connOf(ctx)
	if r.policy.NodeDown.Load() && r.policy.AppHost(conn.Domain) {
		return &decision{Context: ctx, tag: "direct"}, nil
	}
	result := r.policy.Router.Route(conn, func() []netip.Addr {
		if r.dns == nil {
			return nil
		}
		return addrs(routingdns.ContextWithDNSClient(ctx, r.dns).GetTargetIPs())
	})
	tag := r.policy.Final
	if result.Matched {
		tag = outboundTag(result)
	}
	if group, ok := route.CutGroupTag(tag); ok {
		markGroup(ctx, group)
	}
	if r.policy.Resolve != nil {
		tag = r.policy.Resolve(tag)
	}
	return &decision{Context: ctx, tag: tag}, nil
}

// markGroup notes the group on the connection's content, which the dispatcher
// shares with the context the outbound's Dispatch gets.
func markGroup(ctx routing.Context, group string) {
	if rc, ok := ctx.(*routingsession.Context); ok && rc.Content != nil {
		rc.Content.SetAttribute(xrayconf.GroupAttr, group)
	}
}

func (r *xrayRouter) AddRule(*serial.TypedMessage, bool) error {
	return errors.New("rules come from the One Node policy")
}

func (r *xrayRouter) RemoveRule(string) error {
	return errors.New("rules come from the One Node policy")
}

func (r *xrayRouter) ListRule() []routing.Route { return nil }

// fixedRoute places the core's own DNS traffic before any rule: each DoH
// server's queries leave the way its tag says (xrayconf), and DNS that the
// gateway intercepts (LAN clients', and dnsmasq's upstream queries) is
// answered by the core (dns-out), split as the routing says.
// Group delay tests dial one node's outbound by name (xrayconf.ProbeInbound).
func fixedRoute(ctx routing.Context) string {
	if tag, ok := strings.CutPrefix(ctx.GetInboundTag(), xrayconf.ProbeInbound); ok {
		return tag
	}
	switch ctx.GetInboundTag() {
	case xrayconf.DNSRemoteTag:
		return "proxy"
	case xrayconf.DNSDirectTag:
		return "direct"
	case xrayconf.TProxyTag:
		if ctx.GetTargetPort() == 53 {
			return xrayconf.DNSOutTag
		}
	}
	return ""
}

type decision struct {
	routing.Context
	tag string
}

func (d *decision) GetOutboundGroupTags() []string { return nil }

func (d *decision) GetOutboundTag() string { return d.tag }

func (d *decision) GetRuleTag() string { return "" }

// connOf describes a connection for the engine. With routeOnly sniffing (the
// mixed port) the domain is the sniffed name while the target stays the
// dialled address, as sing-box sees TUN connections.
func connOf(ctx routing.Context) *engine.Conn {
	conn := &engine.Conn{
		Domain:   strings.ToLower(ctx.GetTargetDomain()),
		Port:     uint16(ctx.GetTargetPort()),
		Protocol: sniffedProtocol(ctx.GetProtocol()),
	}
	switch ctx.GetNetwork() {
	case xnet.Network_TCP:
		conn.Network = "tcp"
	case xnet.Network_UDP:
		conn.Network = "udp"
	}
	if ips := addrs(ctx.GetTargetIPs()); len(ips) > 0 {
		conn.IP = ips[0]
	}
	return conn
}

// sniffedProtocol names Xray's sniffer results as sing-box does ("http1"
// is HTTP).
func sniffedProtocol(name string) string {
	if strings.HasPrefix(name, "http") {
		return "http"
	}
	return name
}

func addrs(ips []xnet.IP) []netip.Addr {
	out := make([]netip.Addr, 0, len(ips))
	for _, ip := range ips {
		if address, ok := netip.AddrFromSlice(ip); ok {
			out = append(out, address.Unmap())
		}
	}
	return out
}

// outboundTag maps a rule's action to the core's outbounds.
func outboundTag(result engine.Result) string {
	if result.Action == rulespec.ActionReject {
		return "block"
	}
	return result.Outbound
}
