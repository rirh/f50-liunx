package dailylog

import (
	"log"
	"os"
	"path/filepath"
	"slices"
	"testing"
	"time"
)

func TestDailyLogKeepsSevenDays(t *testing.T) {
	dir := t.TempDir()
	now := time.Now()
	touch := func(name string) {
		if err := os.WriteFile(filepath.Join(dir, name), []byte("x\n"), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	day := func(n int) string { return now.AddDate(0, 0, -n).Format(logDayLayout) }
	touch("core.log") // the old single file
	touch("core-" + day(6) + ".log")
	touch("core-" + day(7) + ".log")
	touch("core-" + day(30) + ".log")
	touch("core-tun-" + day(30) + ".log") // another stem
	touch("profile.json")

	d, err := Open(filepath.Join(dir, "core.log"))
	if err != nil {
		t.Fatal(err)
	}
	log.New(d, "", 0).Print("hello")

	// A missing log folder is created.
	if _, err := Open(filepath.Join(dir, "logs", "core.log")); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(dir, "logs", "core-"+day(0)+".log")); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(dir, "logs")); err != nil {
		t.Fatal(err)
	}

	entries, _ := os.ReadDir(dir)
	var got []string
	for _, e := range entries {
		got = append(got, e.Name())
	}
	want := []string{"core-" + day(6) + ".log", "core-" + day(0) + ".log", "core-tun-" + day(30) + ".log", "profile.json"}
	slices.Sort(want)
	if !slices.Equal(got, want) {
		t.Fatalf("files = %v, want %v", got, want)
	}
	raw, _ := os.ReadFile(filepath.Join(dir, "core-"+day(0)+".log"))
	if string(raw) != "hello\n" {
		t.Fatalf("today's file = %q", raw)
	}
}
