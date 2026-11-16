package probe

import (
	"context"
	"net"
	"strconv"
	"testing"
	"time"
)

// TCP and ICMP probes go straight to the server, so loopback stands in for a node.
func TestProbeDirect(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			_ = c.Close()
		}
	}()
	port := ln.Addr().(*net.TCPAddr).Port
	vless := "vless://27a4f0d6-7f1a-4c55-9b0a-0d6f7c1b2e3a@127.0.0.1:" + strconv.Itoa(port) + "?encryption=none&security=tls&type=tcp#t"

	for _, method := range []string{"tcp", "icmp"} {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		ms, err := Node(ctx, vless, Options{Method: method})
		cancel()
		if err != nil {
			t.Fatalf("%s: %v", method, err)
		}
		if ms < 1 {
			t.Fatalf("%s: %d ms", method, ms)
		}
	}

	// Nothing listens on a closed port: tcp must fail rather than report a delay.
	_ = ln.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if _, err := Node(ctx, vless, Options{Method: "tcp"}); err == nil {
		t.Fatal("tcp probe of a closed port succeeded")
	}
}
