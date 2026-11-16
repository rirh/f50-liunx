package probe

import (
	"context"
	"errors"
	"math/rand/v2"
	"net"
	"os"
	"time"

	"f50/proxy/internal/platform"

	"golang.org/x/net/icmp"
	"golang.org/x/net/ipv4"
	"golang.org/x/net/ipv6"
)

// probeICMP times one echo to the server over an unprivileged ICMP socket
// (Linux allows these within net.ipv4.ping_group_range), or a raw one as root:
// OpenWrt's default range admits no group, root's included.
func probeICMP(ctx context.Context, host string) (int64, error) {
	ip, err := resolveHost(ctx, host)
	if err != nil {
		return 0, err
	}
	network, listen, proto := "udp4", "0.0.0.0", 1
	var echo, reply icmp.Type = ipv4.ICMPTypeEcho, ipv4.ICMPTypeEchoReply
	if ip.To4() == nil {
		network, listen, proto = "udp6", "::", 58
		echo, reply = ipv6.ICMPTypeEchoRequest, ipv6.ICMPTypeEchoReply
	}
	raw := false
	conn, err := icmp.ListenPacket(network, listen)
	if err != nil && platform.IsElevated() {
		rawNetwork := "ip4:icmp"
		if proto == 58 {
			rawNetwork = "ip6:ipv6-icmp"
		}
		conn, err = icmp.ListenPacket(rawNetwork, listen)
		raw = true
	}
	if err != nil {
		return 0, err
	}
	defer conn.Close()
	if deadline, ok := ctx.Deadline(); ok {
		_ = conn.SetDeadline(deadline)
	}

	seq, id := rand.IntN(1<<16), os.Getpid()&0xffff
	msg := icmp.Message{Type: echo, Body: &icmp.Echo{ID: id, Seq: seq, Data: []byte("f50-proxy")}}
	packet, err := msg.Marshal(nil)
	if err != nil {
		return 0, err
	}
	start := time.Now()
	var to net.Addr = &net.UDPAddr{IP: ip}
	if raw {
		to = &net.IPAddr{IP: ip}
	}
	if _, err := conn.WriteTo(packet, to); err != nil {
		return 0, err
	}
	buf := make([]byte, 1500)
	for {
		n, peer, err := conn.ReadFrom(buf)
		if err != nil {
			var ne net.Error
			if errors.As(err, &ne) && ne.Timeout() {
				return 0, errors.New("timeout")
			}
			return 0, err
		}
		// Linux rewrites the echo ID on unprivileged sockets, so match on peer
		// and sequence; a raw socket sees every echo reply, so the ID too.
		if !peerIs(peer, ip) {
			continue
		}
		got, err := icmp.ParseMessage(proto, buf[:n])
		if err != nil || got.Type != reply {
			continue
		}
		if body, ok := got.Body.(*icmp.Echo); ok && body.Seq == seq && (!raw || body.ID == id) {
			return max(time.Since(start).Milliseconds(), 1), nil
		}
	}
}

func peerIs(peer net.Addr, ip net.IP) bool {
	switch a := peer.(type) {
	case *net.UDPAddr:
		return a.IP.Equal(ip)
	case *net.IPAddr:
		return a.IP.Equal(ip)
	}
	return false
}
