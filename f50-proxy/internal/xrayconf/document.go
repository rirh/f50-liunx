package xrayconf

// Listen is what reaches the core: the mixed port always, the transparent
// inbound in gateway mode, whose sockets (and every outbound's) carry Mark.
type Listen struct {
	Mode       string
	Port       int
	TProxyPort int
	Mark       int
}

// Document builds the Xray JSON config: one mixed (SOCKS + HTTP) inbound on
// loopback, in gateway mode also the tproxy inbound, the node as "proxy",
// the core's DNS (dnsSection) and traffic counters on the proxy outbound.
// Routing is the core's own router (router.WithPolicy), not an Xray "routing"
// section. The caller appends the other outbounds, then marks them all
// (MarkSockets).
func Document(outbound Obj, l Listen, routing, logLevel string) Obj {
	inbounds := []Obj{{
		"tag": "mixed", "listen": "127.0.0.1", "port": l.Port, "protocol": "socks",
		"settings": Obj{"auth": "noauth", "udp": true, "ip": "127.0.0.1"},
		"sniffing": Obj{"enabled": true, "destOverride": []string{"http", "tls", "quic"}, "routeOnly": true},
	}}
	// In proxy mode direct connections resolve with the system as any program
	// would. In gateway mode they use the core's DNS, IPv4 only (no AAAA
	// either): never a dial to IPv6 outside the node.
	direct := Obj{"tag": "direct", "protocol": "freedom"}
	if l.Mode == ModeGateway {
		inbounds = append(inbounds, Obj{
			"tag": TProxyTag, "listen": "::", "port": l.TProxyPort, "protocol": "dokodemo-door",
			"settings": Obj{"network": "tcp,udp", "followRedirect": true},
			// The mark also goes on the sockets that answer UDP from the
			// original destination (Xray's FakeUDP), so they are never caught again.
			"streamSettings": Obj{"sockopt": Obj{"tproxy": "tproxy", "mark": l.Mark}},
			// No routeOnly: the sniffed name replaces the target, so a direct
			// dial resolves it through the core's DNS.
			"sniffing": Obj{"enabled": true, "destOverride": []string{"http", "tls", "quic"}, "routeOnly": false},
		})
		direct["settings"] = Obj{"domainStrategy": "ForceIPv4"}
	}
	return Obj{
		// Xray itself writes nothing: the log tap files its lines (internal/logtap).
		"log":   Obj{"loglevel": logLevel, "access": "none", "error": "none"},
		"stats": Obj{},
		"policy": Obj{"system": Obj{
			"statsOutboundUplink": true, "statsOutboundDownlink": true,
		}},
		"dns":      dnsSection(l.Mode, routing),
		"inbounds": inbounds,
		"outbounds": []Obj{
			outbound,
			direct,
			{"tag": "block", "protocol": "blackhole"},
			// A and AAAA queries are answered from the core's DNS; other types get an
			// empty answer, so browsers fall back from HTTPS records.
			{"tag": DNSOutTag, "protocol": "dns"},
		},
	}
}

// MarkSockets puts SO_MARK on every outbound but the blackhole (which opens
// none): the node and its front and fragment hops, the other nodes, direct
// and dns-out. The nftables rules return marked packets at once, so the
// core's own connections are never intercepted again.
func MarkSockets(outbounds []Obj, mark int) {
	if mark <= 0 {
		return
	}
	for _, ob := range outbounds {
		if ob["protocol"] != "blackhole" {
			Sockopt(ob)["mark"] = mark
		}
	}
}
