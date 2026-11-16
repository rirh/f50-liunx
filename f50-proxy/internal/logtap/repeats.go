package logtap

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

// What the log keeps about trouble, so a stall can be traced afterwards:
//
//   - every connection that ended in an error, with where it went, through
//     which outbound, how long it lived, what it carried and Xray's reason
//     (Xray itself reports that at info level, below what the file takes);
//   - connections that sent something and got nothing back;
//   - a status line every few minutes: how many connections, how many of them
//     failed, the traffic, the node's delay.
//
// A line that repeats (one host failing over and over, the same DNS error) is
// written once and then counted, and a minute's worth of lines is capped, so
// an hour without network costs a few kilobytes instead of megabytes.
const (
	repeatWindow = time.Minute
	// At most this many trouble lines a minute; the rest are counted.
	TroublePerMinute = 60
)

// repeats decides whether a line is written now or only counted.
type repeats struct {
	mu     sync.Mutex
	seen   map[string]*repeat
	minute time.Time // start of the current budget minute
	spent  int
	over   int // lines dropped since the budget ran out
}

type repeat struct {
	first time.Time
	more  int
}

// allow says whether to write the line keyed key. suffix is what to append:
// how often the same line was held back since it was last written.
func (r *repeats) allow(key string, now time.Time) (write bool, suffix string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.seen == nil {
		r.seen = map[string]*repeat{}
	}
	if e := r.seen[key]; e != nil && now.Sub(e.first) < repeatWindow {
		e.more++
		return false, ""
	}
	if now.Sub(r.minute) >= time.Minute {
		r.minute, r.spent = now, 0
	}
	if r.spent >= TroublePerMinute {
		r.over++
		return false, ""
	}
	r.spent++
	if e := r.seen[key]; e != nil && e.more > 0 {
		suffix = fmt.Sprintf(" (+%d more since %s)", e.more, e.first.Format("15:04:05"))
	}
	if len(r.seen) > 2048 {
		for k, e := range r.seen {
			if now.Sub(e.first) >= repeatWindow && e.more == 0 {
				delete(r.seen, k)
			}
		}
	}
	r.seen[key] = &repeat{first: now}
	return true, suffix
}

// dropped returns and clears how many lines the per-minute cap held back.
func (r *repeats) dropped() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	n := r.over
	r.over = 0
	return n
}

// trouble is the one deduper for connection and Xray trouble lines.
var trouble repeats

// AllowTrouble says whether to write the trouble line keyed key now, and what
// to append to it (see repeats.allow).
func AllowTrouble(key string, now time.Time) (write bool, suffix string) {
	return trouble.allow(key, now)
}

// DroppedTrouble returns and clears how many trouble lines the per-minute cap held back.
func DroppedTrouble() int { return trouble.dropped() }

// repeatKey is a line without what differs between two reports of the same
// thing: Xray's "[1234567] " session id.
func repeatKey(text string) string {
	if strings.HasPrefix(text, "[") {
		if i := strings.Index(text, "] "); i > 0 && i < 14 {
			return text[i+2:]
		}
	}
	return text
}

// worthFiling picks the info lines of Xray that explain a failure; the rest
// of info (one line per connection) stays out of the file.
func worthFiling(text string) bool {
	// The outbound's own report is written by the tracker, with the
	// connection it belongs to (reportEnd).
	if strings.Contains(text, "failed to process outbound traffic") {
		return false
	}
	// A name that does not exist is an answer, not trouble; when a lookup
	// really fails Xray says so at error level.
	if strings.Contains(text, "failed to lookup ip for domain") {
		return false
	}
	return strings.Contains(text, "failed to ") || strings.Contains(text, "rejected")
}
