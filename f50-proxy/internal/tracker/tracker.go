// Package tracker counts and lists every connection through the running
// instance (the 连接 page) and logs the ones that end badly.
package tracker

import (
	"cmp"
	"context"
	"net/netip"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"f50/proxy/internal/logtap"
	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/common"
	"github.com/xtls/xray-core/common/buf"
	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/common/session"
	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/features/outbound"
	xtransport "github.com/xtls/xray-core/transport"
)

// ---- Connections ----

// Conn is one connection through the running instance, as the 连接 page shows it.
type Conn struct {
	ID       uint64 `json:"id"`
	Start    int64  `json:"start"` // Unix ms
	End      int64  `json:"end"`   // Unix ms, 0 while open
	Network  string `json:"network"`
	Host     string `json:"host"`         // the domain (requested or sniffed), else the IP
	IP       string `json:"ip,omitempty"` // the IP the client dialled, when Host is a sniffed domain
	Port     uint16 `json:"port"`
	Protocol string `json:"protocol,omitempty"` // sniffed: tls, http1, quic …
	Inbound  string `json:"inbound"`            // mixed | tproxy
	Outbound string `json:"outbound"`           // proxy | direct | block | node:<name>
	Group    string `json:"group,omitempty"`    // the 策略组 that picked Outbound
	Up       int64  `json:"up"`
	Down     int64  `json:"down"`
	// Source is the client's address (IP only): a LAN device, or the router
	// itself (its own address, or loopback through the mixed port).
	Source string `json:"source"`
	// Last is when a byte last went either way (Unix ms, 0: never), for the
	// log: an error after a long silence is an idle connection's end.
	Last int64 `json:"-"`
}

// liveConn is an open Conn with its byte counters and what ends it: the
// link, and the outbound's context. Inbounds that dispatch a link (mixed,
// tproxy) hand the outbound the client socket itself, which interrupting the
// link does not reach; cancelling the context ends the outbound, and the
// inbound closes the socket once Dispatch returns.
type liveConn struct {
	Conn
	up, down atomic.Int64
	last     atomic.Int64 // Conn.Last
	link     *xtransport.Link
	cancel   context.CancelFunc
	closed   atomic.Bool // ended by Close, not by itself
}

const closedKept = 200

// Tracker sees every connection by wrapping the instance's outbound handlers:
// Dispatch runs for the connection's whole life, and the link it carries is
// counted on the way.
type Tracker struct {
	seq    atomic.Uint64
	mu     sync.Mutex
	open   map[uint64]*liveConn
	closed []Conn // oldest first
	// sent is each wrapped outbound's upload this run (payload bytes).
	sent map[string]*atomic.Int64
	// totals since the last status line (StatusLine).
	totals connTotals
}

func New() *Tracker {
	return &Tracker{open: map[uint64]*liveConn{}}
}

// Wrap replaces the instance's outbounds with counted ones. Called after
// core.New and before Start; tags are re-added in order so the first outbound
// ("proxy") stays the default handler.
func (t *Tracker) Wrap(inst *core.Instance, tags ...string) error {
	om, ok := inst.GetFeature(outbound.ManagerType()).(outbound.Manager)
	if !ok {
		return nil
	}
	ctx := context.Background()
	sent := map[string]*atomic.Int64{}
	for _, tag := range tags {
		sent[tag] = new(atomic.Int64)
	}
	t.mu.Lock()
	t.sent = sent
	t.mu.Unlock()
	handlers := make([]outbound.Handler, 0, len(tags))
	for _, tag := range tags {
		if h := om.GetHandler(tag); h != nil {
			handlers = append(handlers, h)
			if err := om.RemoveHandler(ctx, tag); err != nil {
				return err
			}
		}
	}
	for _, h := range handlers {
		if err := om.AddHandler(ctx, &trackedHandler{Handler: h, tracker: t, sent: sent[h.Tag()]}); err != nil {
			return err
		}
	}
	return nil
}

type trackedHandler struct {
	outbound.Handler
	tracker *Tracker
	sent    *atomic.Int64
}

// Sent is the upload through the outbound tagged tag since the last Wrap.
func (t *Tracker) Sent(tag string) *atomic.Int64 {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.sent[tag]
}

func (h *trackedHandler) Dispatch(ctx context.Context, link *xtransport.Link) {
	if in := session.InboundFromContext(ctx); in != nil && strings.HasPrefix(in.Tag, xrayconf.ProbeInbound) {
		h.Handler.Dispatch(ctx, link) // a group's delay test, not the user's traffic
		return
	}
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	// What the outbound says went wrong, for the log (connlog.go, reportEnd).
	failure := &connError{}
	ctx = session.TrackedConnectionError(ctx, failure)
	c := h.tracker.add(ctx, h.Tag(), link, cancel)
	defer func() { h.tracker.done(c, failure.get()) }()
	reader := countReader(link.Reader, &c.up, &c.last)
	if h.sent != nil {
		reader = countReader(reader, h.sent, nil)
	}
	h.Handler.Dispatch(ctx, &xtransport.Link{Reader: reader, Writer: &countedWriter{link.Writer, &c.down, &c.last}})
}

func (t *Tracker) add(ctx context.Context, tag string, link *xtransport.Link, cancel context.CancelFunc) *liveConn {
	c := &liveConn{Conn: Conn{ID: t.seq.Add(1), Start: time.Now().UnixMilli(), Outbound: tag}, link: link, cancel: cancel}
	if in := session.InboundFromContext(ctx); in != nil {
		c.Inbound = in.Tag
		c.Source = sourceIP(in.Source)
	}
	if obs := session.OutboundsFromContext(ctx); len(obs) > 0 {
		ob := obs[len(obs)-1]
		dest := ob.Target
		// routeOnly sniffing (the mixed inbound) keeps the dialled address as the
		// target and puts the sniffed domain here.
		if ob.RouteTarget.Address != nil && ob.RouteTarget.Address.Family().IsDomain() {
			dest = ob.RouteTarget
		}
		c.Network, c.Port = dest.Network.SystemString(), uint16(dest.Port)
		if dest.Address != nil {
			c.Host = dest.Address.String()
			if dest.Address.Family().IsDomain() && ob.OriginalTarget.Address != nil && ob.OriginalTarget.Address.Family().IsIP() {
				c.IP = ob.OriginalTarget.Address.String()
			}
		}
	}
	if content := session.ContentFromContext(ctx); content != nil {
		c.Protocol = content.Protocol
		c.Group = content.Attribute(xrayconf.GroupAttr)
	}
	t.mu.Lock()
	t.open[c.ID] = c
	t.mu.Unlock()
	return c
}

// sourceIP is a connection's client address without the port; IPv4-mapped
// IPv6 (the dual-stack tproxy socket) is written as IPv4.
func sourceIP(src xnet.Destination) string {
	if src.Address == nil || !src.Address.Family().IsIP() {
		return ""
	}
	if ip, ok := netip.AddrFromSlice(src.Address.IP()); ok {
		return ip.Unmap().String()
	}
	return src.Address.String()
}

func (t *Tracker) done(c *liveConn, err error) {
	ended := c.snapshot()
	ended.End = time.Now().UnixMilli()
	// Closed by us (a stop, a node switch, a network change): not the connection's fault.
	if c.closed.Load() {
		err = nil
	}
	t.reportEnd(ended, err)
	t.mu.Lock()
	defer t.mu.Unlock()
	delete(t.open, c.ID)
	if len(t.closed) >= closedKept {
		t.closed = slices.Delete(t.closed, 0, len(t.closed)-closedKept+1)
	}
	t.closed = append(t.closed, ended)
}

func (c *liveConn) snapshot() Conn {
	s := c.Conn
	s.Up, s.Down, s.Last = c.up.Load(), c.down.Load(), c.last.Load()
	return s
}

// Connections lists open connections (oldest first) and the latest closed ones (newest first).
func (t *Tracker) Connections() map[string]any {
	t.mu.Lock()
	defer t.mu.Unlock()
	open := make([]Conn, 0, len(t.open))
	for _, c := range t.open {
		open = append(open, c.snapshot())
	}
	slices.SortFunc(open, func(a, b Conn) int { return cmp.Compare(a.ID, b.ID) })
	// Never nil: before the first connection ends the page must get [] (JSON), not null.
	closed := append(make([]Conn, 0, len(t.closed)), t.closed...)
	slices.Reverse(closed)
	return map[string]any{"epoch": logtap.Epoch, "active": open, "closed": closed}
}

// Close interrupts one open connection, or all of them when id is 0.
func (t *Tracker) Close(id uint64) int {
	return t.CloseWhere(func(c *Conn) bool { return id == 0 || c.ID == id })
}

// CloseWhere interrupts the open connections match picks.
func (t *Tracker) CloseWhere(match func(*Conn) bool) int {
	t.mu.Lock()
	var hit []*liveConn
	for _, c := range t.open {
		if match(&c.Conn) {
			hit = append(hit, c)
		}
	}
	t.mu.Unlock()
	for _, c := range hit {
		c.closed.Store(true)
		c.cancel()
		_ = common.Interrupt(c.link.Reader)
		_ = common.Interrupt(c.link.Writer)
	}
	return len(hit)
}

// Some outbounds type-assert buf.TimeoutReader, so the counted reader only
// claims it when the wrapped one has it. last (may be nil) takes the time of
// each read that carried bytes.
func countReader(r buf.Reader, n, last *atomic.Int64) buf.Reader {
	c := countedReader{r, n, last}
	if tr, ok := r.(buf.TimeoutReader); ok {
		return &countedTimeoutReader{c, tr}
	}
	return &c
}

type countedReader struct {
	buf.Reader
	n, last *atomic.Int64
}

// count adds k bytes and, when there were any, stamps last.
func count(n, last *atomic.Int64, k int32) {
	if k == 0 {
		return
	}
	n.Add(int64(k))
	if last != nil {
		last.Store(time.Now().UnixMilli())
	}
}

func (r *countedReader) ReadMultiBuffer() (buf.MultiBuffer, error) {
	mb, err := r.Reader.ReadMultiBuffer()
	count(r.n, r.last, mb.Len())
	return mb, err
}

func (r *countedReader) Interrupt() { _ = common.Interrupt(r.Reader) }

func (r *countedReader) Close() error { return common.Close(r.Reader) }

type countedTimeoutReader struct {
	countedReader
	tr buf.TimeoutReader
}

func (r *countedTimeoutReader) ReadMultiBufferTimeout(d time.Duration) (buf.MultiBuffer, error) {
	mb, err := r.tr.ReadMultiBufferTimeout(d)
	count(r.n, r.last, mb.Len())
	return mb, err
}

type countedWriter struct {
	buf.Writer
	n, last *atomic.Int64
}

func (w *countedWriter) WriteMultiBuffer(mb buf.MultiBuffer) error {
	count(w.n, w.last, mb.Len())
	return w.Writer.WriteMultiBuffer(mb)
}

func (w *countedWriter) Interrupt() { _ = common.Interrupt(w.Writer) }

func (w *countedWriter) Close() error { return common.Close(w.Writer) }
