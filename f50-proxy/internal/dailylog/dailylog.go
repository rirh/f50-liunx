// Package dailylog is the file side of serve --log: one file per local day, the
// last few days kept.
package dailylog

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// logDaysKept is how many local days of log files stay on disk, today included.
const logDaysKept = 7

const logDayLayout = "2006-01-02"

// Log is the file side of --log: for a path like dir/core.log it appends to
// dir/core-2026-10-31.log, moves to a new file when the local day changes, and
// deletes files of the same stem older than logDaysKept days.
type Log struct {
	dir, stem string

	mu  sync.Mutex
	day string
	f   *os.File
}

func Open(path string) (*Log, error) {
	d := &Log{dir: filepath.Dir(path), stem: strings.TrimSuffix(filepath.Base(path), filepath.Ext(path))}
	if err := os.MkdirAll(d.dir, 0o755); err != nil {
		return nil, err
	}
	d.mu.Lock()
	defer d.mu.Unlock()
	if err := d.rollLocked(time.Now()); err != nil {
		return nil, err
	}
	// The single ever-growing file earlier versions wrote.
	_ = os.Remove(path)
	return d, nil
}

func (d *Log) Write(p []byte) (int, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if now := time.Now(); now.Format(logDayLayout) != d.day {
		if err := d.rollLocked(now); err != nil && d.f == nil {
			return 0, err
		}
	}
	return d.f.Write(p)
}

// rollLocked opens today's file; on failure the previous file (if any) stays in use.
func (d *Log) rollLocked(now time.Time) error {
	day := now.Format(logDayLayout)
	name := filepath.Join(d.dir, d.stem+"-"+day+".log")
	f, err := os.OpenFile(name, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return err
	}
	if d.f != nil {
		_ = d.f.Close()
	}
	d.f, d.day = f, day
	d.prune(now)
	return nil
}

func (d *Log) prune(now time.Time) {
	entries, err := os.ReadDir(d.dir)
	if err != nil {
		return
	}
	y, m, dd := now.Date()
	cutoff := time.Date(y, m, dd-(logDaysKept-1), 0, 0, 0, 0, time.Local)
	for _, e := range entries {
		day, ok := d.fileDay(e.Name())
		if ok && day.Before(cutoff) {
			_ = os.Remove(filepath.Join(d.dir, e.Name()))
		}
	}
}

// fileDay parses <stem>-YYYY-MM-DD.log; other files don't match.
func (d *Log) fileDay(name string) (time.Time, bool) {
	rest, ok := strings.CutPrefix(name, d.stem+"-")
	if !ok {
		return time.Time{}, false
	}
	rest, ok = strings.CutSuffix(rest, ".log")
	if !ok {
		return time.Time{}, false
	}
	day, err := time.ParseInLocation(logDayLayout, rest, time.Local)
	return day, err == nil
}
