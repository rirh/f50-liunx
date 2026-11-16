package engine

import (
	"context"
	"fmt"
	"strings"
	"time"

	"f50/proxy/internal/link"
	"f50/proxy/internal/logtap"
	"f50/proxy/internal/netmark"

	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/common/protocol"
	"github.com/xtls/xray-core/common/serial"
	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/proxy/http"
	"github.com/xtls/xray-core/proxy/hysteria"
	"github.com/xtls/xray-core/proxy/shadowsocks"
	"github.com/xtls/xray-core/proxy/shadowsocks_2022"
	"github.com/xtls/xray-core/proxy/socks"
	"github.com/xtls/xray-core/proxy/trojan"
	vlessout "github.com/xtls/xray-core/proxy/vless/outbound"
	vmessout "github.com/xtls/xray-core/proxy/vmess/outbound"
)

// resolveServers swaps proxy server domains for IPv4 addresses, so Xray never
// resolves its own server through the interception (the core's DNS would ask
// through the node). SNI fields stay intact.
func resolveServers(config *core.Config) error {
	for _, ob := range config.Outbound {
		if ob.ProxySettings == nil {
			continue
		}
		// Only the picked node and its front nodes must resolve; a group's node
		// that does not is left to the core's DNS and its delay test.
		if ob.Tag != "proxy" && !strings.HasPrefix(ob.Tag, "proxy"+link.ViaSuffix) {
			if err := resolveServer(ob); err != nil {
				logtap.Note(fmt.Sprintf("outbound %q: %v", ob.Tag, err))
			}
			continue
		}
		if err := resolveServer(ob); err != nil {
			return err
		}
	}
	return nil
}

func resolveServer(ob *core.OutboundHandlerConfig) error {
	inst, err := ob.ProxySettings.GetInstance()
	if err != nil {
		return fmt.Errorf("outbound %q: %w", ob.Tag, err)
	}
	var address **xnet.IPOrDomain
	endpoint := func(s *protocol.ServerEndpoint) {
		if s != nil {
			address = &s.Address
		}
	}
	switch c := inst.(type) {
	case *vlessout.Config:
		endpoint(c.Vnext)
	case *vmessout.Config:
		endpoint(c.Receiver)
	case *hysteria.ClientConfig:
		endpoint(c.Server)
	case *trojan.ClientConfig:
		endpoint(c.Server)
	case *shadowsocks.ClientConfig:
		endpoint(c.Server)
	case *shadowsocks_2022.ClientConfig:
		address = &c.Address
	case *socks.ClientConfig:
		endpoint(c.Server)
	case *http.ClientConfig:
		endpoint(c.Server)
	default:
		return nil
	}
	if address == nil || *address == nil || (*address).GetDomain() == "" {
		return nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	ips, err := netmark.Resolver().LookupIP(ctx, "ip4", (*address).GetDomain())
	cancel()
	if err != nil || len(ips) == 0 {
		return fmt.Errorf("resolve node server: %v", err)
	}
	*address = xnet.NewIPOrDomain(xnet.IPAddress(ips[0]))
	ob.ProxySettings = serial.ToTypedMessage(inst)
	return nil
}
