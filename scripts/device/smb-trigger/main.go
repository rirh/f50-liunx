// Access one F50 share through a userspace SMB2/3 Guest session. No OS mount,
// file writes, downloaded code, or network access beyond the requested server.
package main

import (
	"context"
	"encoding/binary"
	"flag"
	"fmt"
	"log"
	"math"
	"net"
	"os"
	"strings"
	"time"

	"github.com/hirochachacha/go-smb2"
)

type traceConn struct {
	net.Conn
	label      string
	negotiate  int
	setup      int
}

func (conn *traceConn) Write(packet []byte) (int, error) {
	// go-smb2 writes the four-byte TCP frame length separately, then the SMB
	// packet. Log only its command number, never authentication payloads.
	if len(packet) >= 64 && string(packet[:4]) == "\xfeSMB" {
		switch binary.LittleEndian.Uint16(packet[12:14]) {
		case 0:
			conn.negotiate++
			log.Printf("SMB %s send NEGOTIATE #%d (command=0)", conn.label, conn.negotiate)
		case 1:
			conn.setup++
			log.Printf("SMB %s send SESSION_SETUP #%d (command=1)", conn.label, conn.setup)
		}
	}
	return conn.Conn.Write(packet)
}

func trigger(address string, seconds float64, shares []string) error {
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(seconds*float64(time.Second)))
	defer cancel()
	sessionFailed, err := triggerAttempt(ctx, address, shares, 0, "auto")
	if err == nil || !sessionFailed || ctx.Err() != nil {
		return err
	}
	log.Printf("Automatic SMB session failed: %v", err)
	log.Print("Trying SMB 2.1 once within the same deadline; compatibility attempt, not a confirmed cause")
	// v1.1.0 Negotiator supports SpecifiedDialect=SMB210 (0x0210).
	_, err = triggerAttempt(ctx, address, shares, 0x0210, "SMB2.1")
	return err
}

// sessionFailed is true only before a session exists, so a dialect retry never
// repeats share access or a root preexec that might already have run.
func triggerAttempt(ctx context.Context, address string, shares []string, dialect uint16, label string) (sessionFailed bool, err error) {
	log.Printf("SMB %s TCP connect %s", label, address)
	conn, err := (&net.Dialer{}).DialContext(ctx, "tcp", address)
	if err != nil {
		return false, fmt.Errorf("TCP connect: %w", err)
	}
	defer conn.Close()
	deadline, _ := ctx.Deadline()
	if err := conn.SetDeadline(deadline); err != nil {
		return false, fmt.Errorf("TCP deadline: %w", err)
	}
	// Also close the connection at the deadline, including library cleanup paths.
	timer := time.AfterFunc(time.Until(deadline), func() { _ = conn.Close() })
	defer timer.Stop()
	log.Printf("SMB %s negotiate and authenticate Guest", label)
	dialer := &smb2.Dialer{
		Negotiator: smb2.Negotiator{SpecifiedDialect: dialect},
		Initiator:  &smb2.NTLMInitiator{User: "Guest", Password: ""},
	}
	session, err := dialer.DialContext(ctx, &traceConn{Conn: conn, label: label})
	if err != nil {
		return true, fmt.Errorf("SMB %s Guest session: %w", label, err)
	}
	// v1 sessions and shares do not inherit the caller's context automatically.
	session = session.WithContext(ctx)
	defer func() {
		if err := session.Logoff(); err != nil {
			log.Printf("SMB logoff: %v", err)
		}
	}()
	if len(shares) == 0 {
		log.Print("SMB list shares")
		shares, err = session.ListSharenames()
		if err != nil {
			return false, fmt.Errorf("SMB list shares: %w", err)
		}
	}
	for _, name := range shares {
		if err := ctx.Err(); err != nil {
			return false, err
		}
		switch strings.ToUpper(name) {
		case "IPC$", "ADMIN$", "C$":
			continue
		}
		if name == "" || strings.ContainsAny(name, `\/`) {
			continue
		}
		log.Printf("SMB tree connect %q", name)
		share, err := session.Mount(name)
		if err != nil {
			log.Printf("SMB tree connect %q: %v", name, err)
			continue
		}
		share = share.WithContext(ctx)
		log.Printf("SMB stat %q root directory", name)
		_, statErr := share.Stat(".")
		if err := share.Umount(); err != nil {
			log.Printf("SMB tree disconnect %q: %v", name, err)
		}
		if statErr == nil {
			return false, nil
		}
		log.Printf("SMB stat %q: %v", name, statErr)
	}
	return false, fmt.Errorf("no regular share could be accessed")
}

func main() {
	address := flag.String("address", "192.168.0.1:445", "target IPv4 address and SMB port")
	seconds := flag.Float64("timeout", 6, "total session deadline in seconds (0 < seconds <= 60)")
	var shares []string
	flag.Func("share", "known share name; may be repeated to skip share enumeration", func(name string) error {
		if name == "" || strings.ContainsAny(name, `\/`) {
			return fmt.Errorf("share must be a name, not a path")
		}
		shares = append(shares, name)
		return nil
	})
	flag.Parse()
	log.SetFlags(log.Ltime)
	host, _, err := net.SplitHostPort(*address)
	if err != nil || net.ParseIP(host).To4() == nil || flag.NArg() != 0 ||
		*seconds <= 0 || *seconds > 60 || math.IsNaN(*seconds) || math.IsInf(*seconds, 0) {
		fmt.Fprintln(os.Stderr, "invalid address, timeout, or positional arguments")
		os.Exit(2)
	}
	if err := trigger(*address, *seconds, shares); err != nil {
		log.Printf("SMB trigger failed: %v", err)
		os.Exit(1)
	}
	fmt.Println("F50_SMB_TRIGGERED")
}
