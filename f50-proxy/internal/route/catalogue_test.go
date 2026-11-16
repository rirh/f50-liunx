package route

import (
	"net/http"
	"net/http/httptest"
	"os"
	"sync/atomic"
	"testing"
)

// A rule set fetched once is kept in the cache dir and serves the next
// process when the catalogue is unreachable.
func TestCatalogueDiskCache(t *testing.T) {
	body, err := os.ReadFile("../geodata/rules/apps/geosite-openai.json")
	if err != nil {
		t.Fatal(err)
	}
	hits := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		if r.URL.Path != "/rules/geosite-x.json" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("ETag", `"v1"`)
		_, _ = w.Write(body)
	}))
	saved := CacheDir
	CacheDir = t.TempDir()
	defer func() { CacheDir = saved }()
	fresh := func() *Catalogue {
		cataloguesMu.Lock()
		clear(catalogues)
		cataloguesMu.Unlock()
		return CatalogueAt(srv.URL + "/rules")
	}

	rules, err := fresh().Load("geosite-x")
	if err != nil || len(rules) == 0 || hits != 1 {
		t.Fatalf("first load: %d rules, %v, %d hits", len(rules), err, hits)
	}
	// A new process within catalogueFresh reads the disk without asking.
	if again, err := fresh().Load("geosite-x"); err != nil || len(again) != len(rules) || hits != 1 {
		t.Fatalf("cached load: %v, %d hits", err, hits)
	}
	// Stale and unreachable: the kept copy still serves.
	c := fresh()
	old := c.readCache("geosite-x")
	if old == nil || old.etag != `"v1"` {
		t.Fatalf("cache entry %+v", old)
	}
	stale := old.checked.Add(-2 * catalogueFresh)
	if err := os.Chtimes(c.cacheFile("geosite-x", ".json"), stale, stale); err != nil {
		t.Fatal(err)
	}
	srv.Close()
	if kept, err := c.Load("geosite-x"); err != nil || len(kept) != len(rules) {
		t.Fatalf("offline load: %v", err)
	}
	if _, err := fresh().Load("geosite-missing"); err == nil {
		t.Fatal("an uncached tag offline must fail")
	}
}

// A catalogue set answers from memory for hours; a stale entry revalidates
// with its ETag.
func TestCatalogueRevalidates(t *testing.T) {
	var requests, revalidations atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		if r.URL.Path != "/api/v1/one/rules/geosite-example.json" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("ETag", `"v1"`)
		if r.Header.Get("If-None-Match") == `"v1"` {
			revalidations.Add(1)
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Write([]byte(`{"version": 2, "rules": [{"domain_suffix": ["example.com", "example.net"]}]}`))
	}))
	defer server.Close()
	remote := CatalogueAt(server.URL + "/api/v1/one/rules")
	text := `[{"rule_set": "geosite-example", "outbound": "proxy"}, {"rule_set": "geosite-missing", "outbound": "direct"}, {"rule_set": "geoip-cn", "outbound": "proxy"}]`
	rt, err := CompileRoute(text, remote)
	if err != nil {
		t.Fatal(err)
	}
	if rt.Source != 2 {
		t.Fatalf("source %d warnings %v", rt.Source, rt.Warnings)
	}
	remote.entries["geosite-example"].checked = remote.entries["geosite-example"].checked.Add(-2 * catalogueFresh)
	before := requests.Load()
	if _, err := CompileRoute(text, remote); err != nil {
		t.Fatal(err)
	}
	if revalidations.Load() != 1 || requests.Load()-before != 2 {
		t.Errorf("requests %d, revalidations %d", requests.Load()-before, revalidations.Load())
	}
}
