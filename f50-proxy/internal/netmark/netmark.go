// Package netmark gives the engine's own sockets (rule-set downloads, probes,
// resolving node servers) the running gateway's SO_MARK, and resolves through
// bootstrap DNS servers while one is set.
package netmark

import (
	"context"
	"net"
	"sync/atomic"
	"time"
)

// socketMark is the SO_MARK of the gateway being started or running (0
// otherwise). Xray's own sockets get it from the config (xrayconf.MarkSockets); the
// ones the core opens itself (rule-set downloads, probes, resolving node
// servers) take it from here, so the nftables rules let them pass too.
var socketMark atomic.Int32

// Set is the mark of the gateway being started (0: none).
func Set(mark int) { socketMark.Store(int32(mark)) }

// Get is the mark the engine's own sockets carry now.
func Get() int { return int(socketMark.Load()) }

// bootstrapDNS answers the core's own lookups in gateway mode. The system
// resolver is dnsmasq there, and its upstream queries are sent to the core's
// DNS by the nftables rules: not up yet while the core starts, gone after a
// crash, and a node server's name must never be resolved through the node.
var bootstrapDNS = []string{"223.5.5.5:53", "119.29.29.29:53"}

var bootstrapNext atomic.Uint32

// Dialer opens sockets carrying the mark, resolving names with
// Resolver.
func Dialer(timeout time.Duration) *net.Dialer {
	d := &net.Dialer{Timeout: timeout, Resolver: Resolver()}
	if m := socketMark.Load(); m > 0 {
		d.Control = markControl(int(m))
	}
	return d
}

// Dial is an http.Transport's DialContext over Dialer.
func Dial(ctx context.Context, network, address string) (net.Conn, error) {
	return Dialer(10*time.Second).DialContext(ctx, network, address)
}

// Resolver is the system resolver, or in gateway mode the bootstrap
// servers over marked sockets.
func Resolver() *net.Resolver {
	m := socketMark.Load()
	if m <= 0 {
		return net.DefaultResolver
	}
	return &net.Resolver{PreferGo: true, Dial: func(ctx context.Context, network, _ string) (net.Conn, error) {
		d := net.Dialer{Timeout: 3 * time.Second, Control: markControl(int(m))}
		server := bootstrapDNS[int(bootstrapNext.Add(1))%len(bootstrapDNS)]
		return d.DialContext(ctx, network, server)
	}}
}
