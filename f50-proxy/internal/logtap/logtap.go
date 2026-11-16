// Package logtap is the process's logger: Xray's log handler and the engine's
// own notes, filed through the standard logger (trouble deduplicated) and kept
// for the 日志 page.
package logtap

import (
	"cmp"
	"io"
	stdlog "log"
	"os"
	"slices"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	applog "github.com/xtls/xray-core/app/log"
	"github.com/xtls/xray-core/common/log"
	"github.com/xtls/xray-core/common/serial"
	"github.com/xtls/xray-core/core"
)

// Epoch tells f50d's pollers that the engine process changed (a restart), so
// log sequence numbers and connection ids start over.
var Epoch = strconv.FormatInt(time.Now().UnixNano(), 36)

// ---- Log ----

// LogLine is one line of the 日志 page.
type LogLine struct {
	Seq   uint64 `json:"seq"`
	Time  int64  `json:"time"`  // Unix ms
	Run   int64  `json:"run"`   // ms since the current connection started (before the first: since the process did)
	Level string `json:"level"` // error | warning | info
	Text  string `json:"text"`
}

const linesKept = 1000

// logTap is Xray's process-wide log handler. It files Xray's lines at the
// configured level through the standard logger (the daily core-<date>.log, see
// internal/dailylog) and keeps the latest info-and-above lines for the page.
// Every core.New replaces the handler, so the engine re-installs the tap after each.
type logTap struct {
	severity atomic.Int32 // what Xray should bother formatting; read on every log call
	fileSev  atomic.Int32 // what goes to the log file: the instance's loglevel
	runStart atomic.Int64 // Unix ms the lines' run clock counts from

	mu    sync.Mutex
	seq   uint64
	lines []LogLine // ring, len ≤ linesKept
}

// tap is the process's one log tap: the logger every package writes through.
var tap = newLogTap()

// Note records a line of our own (connect, stop) next to Xray's.
func Note(text string) { tap.Note(text) }

// Warn records a line of our own that reports trouble: filed on the trouble
// logger (SetTroubleOutput) and kept as a warning for the page.
func Warn(text string) { tap.Warn(text) }

// troubleLog files Xray's warnings and errors and our Warn lines. It writes
// where the standard logger does unless SetTroubleOutput splits the two: under
// procd the standard logger goes to stdout (logd's daemon.info) and this one
// to stderr (daemon.err), so syslog's priority says which lines are trouble.
var troubleLog = stdlog.New(os.Stderr, "", stdlog.LstdFlags|stdlog.Lmicroseconds)

// SetTroubleOutput sets where trouble lines go (see troubleLog).
func SetTroubleOutput(w io.Writer) { troubleLog.SetOutput(w) }

// Install makes the tap Xray's handler again, filing at inst's loglevel (nil: keep the last).
func Install(inst *core.Instance) { tap.install(inst) }

// Reinstall puts the tap back after another instance (a probe) replaced it.
func Reinstall() { tap.reinstall() }

// Since returns the lines after seq, oldest first (logTap.Since).
func Since(callerEpoch string, after uint64) map[string]any { return tap.Since(callerEpoch, after) }

func newLogTap() *logTap {
	t := &logTap{}
	t.severity.Store(int32(log.Severity_Info))
	t.fileSev.Store(int32(log.Severity_Warning))
	t.runStart.Store(time.Now().UnixMilli())
	return t
}

// install makes the tap Xray's handler again, filing at inst's loglevel (nil: keep the last).
func (t *logTap) install(inst *core.Instance) {
	if inst != nil {
		t.runStart.Store(time.Now().UnixMilli())
		if l, ok := inst.GetFeature((*applog.Instance)(nil)).(*applog.Instance); ok {
			t.fileSev.Store(int32(l.Severity()))
		}
	}
	t.severity.Store(int32(max(log.Severity_Info, log.Severity(t.fileSev.Load()))))
	log.RegisterHandler(t)
}

// reinstall puts the tap back after another instance (a probe) replaced it.
func (t *logTap) reinstall() { log.RegisterHandler(t) }

func (t *logTap) Severity() log.Severity { return log.Severity(t.severity.Load()) }

func (t *logTap) Handle(msg log.Message) {
	m, ok := msg.(*log.GeneralMessage)
	if !ok || m.Severity == log.Severity_Unknown {
		return
	}
	text := serial.ToString(m.Content)
	// The file takes the instance's loglevel, plus the info lines that explain
	// a failure; repeats are counted, not written (repeats.go).
	if m.Severity <= log.Severity(t.fileSev.Load()) || (m.Severity == log.Severity_Info && worthFiling(text)) {
		// Only trouble is folded: a restart's "started" lines are each written.
		write, suffix := true, ""
		if m.Severity == log.Severity_Error || strings.Contains(text, "fail") {
			write, suffix = trouble.allow(repeatKey(text), time.Now())
		}
		if write {
			logger := stdlog.Default()
			if m.Severity <= log.Severity_Warning {
				logger = troubleLog
			}
			logger.Printf("[%s] %s%s", m.Severity, text, suffix)
		}
	}
	if m.Severity <= log.Severity_Info {
		t.add(m.Severity, text)
	}
}

// Note files and keeps one line of our own.
func (t *logTap) Note(text string) {
	stdlog.Printf("f50-proxy: %s", text)
	t.add(log.Severity_Info, "f50-proxy: "+text)
}

// Warn files and keeps one trouble line of our own.
func (t *logTap) Warn(text string) {
	troubleLog.Printf("f50-proxy: %s", text)
	t.add(log.Severity_Warning, "f50-proxy: "+text)
}

func (t *logTap) add(sev log.Severity, text string) {
	level := "info"
	switch sev {
	case log.Severity_Error:
		level = "error"
	case log.Severity_Warning:
		level = "warning"
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	t.seq++
	now := time.Now().UnixMilli()
	line := LogLine{Seq: t.seq, Time: now, Run: now - t.runStart.Load(), Level: level, Text: text}
	if len(t.lines) < linesKept {
		t.lines = append(t.lines, line)
	} else {
		t.lines[(t.seq-1)%linesKept] = line
	}
}

// Since returns the lines after seq, oldest first. A caller from another epoch
// (another engine process) gets everything kept.
func (t *logTap) Since(callerEpoch string, after uint64) map[string]any {
	if callerEpoch != Epoch {
		after = 0
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	out := make([]LogLine, 0, 64)
	for _, l := range t.lines {
		if l.Seq > after {
			out = append(out, l)
		}
	}
	slices.SortFunc(out, func(a, b LogLine) int { return cmp.Compare(a.Seq, b.Seq) })
	return map[string]any{"epoch": Epoch, "next": t.seq, "lines": out}
}
