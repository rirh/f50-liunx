package route

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"f50/proxy/internal/netmark"
	"f50/proxy/internal/rulespec"
	"f50/proxy/internal/rulespec/engine"
	"f50/proxy/internal/version"
)

// Rule sets a route can reference: inline ones from the 配置 tab, the
// built-in lists (geodata) (offline, curated by cmd/genrules), and
// the One Node rule catalogue for everything else.

// CacheDir keeps the catalogue's rule sets across restarts (serve
// --cache-dir; empty: memory only), so a boot without network, or a restart
// while the interception is up and nothing answers it yet, still has them.
var CacheDir = "/etc/f50d/proxy/rulesets"

// Catalogue downloads rule sets as sing-box source (<tag>.json) from a One
// Node backend, revalidating with ETag; a stale copy (in memory, else in
// CacheDir) serves when the backend is unreachable.
type Catalogue struct {
	base   string
	dir    string // CacheDir/<hash of base>, or "" for no disk cache
	client *http.Client

	mu      sync.Mutex
	entries map[string]*catalogueEntry
}

type catalogueEntry struct {
	etag    string
	rules   []rulespec.Match
	checked time.Time
}

const (
	catalogueFresh   = 6 * time.Hour
	catalogueTimeout = 4 * time.Second
)

var (
	cataloguesMu sync.Mutex
	catalogues   = map[string]*Catalogue{}
)

// CatalogueAt shares one cache per backend across starts and checks.
func CatalogueAt(base string) *Catalogue {
	base = strings.TrimSpace(base)
	if base == "" {
		return nil
	}
	if !strings.HasSuffix(base, "/") {
		base += "/"
	}
	cataloguesMu.Lock()
	defer cataloguesMu.Unlock()
	if c, ok := catalogues[base]; ok {
		return c
	}
	c := &Catalogue{
		base:    base,
		client:  &http.Client{Timeout: catalogueTimeout, Transport: &http.Transport{Proxy: http.ProxyFromEnvironment, DialContext: netmark.Dial}},
		entries: map[string]*catalogueEntry{},
	}
	if CacheDir != "" {
		sum := sha256.Sum256([]byte(base))
		c.dir = filepath.Join(CacheDir, hex.EncodeToString(sum[:6]))
	}
	catalogues[base] = c
	return c
}

func (c *Catalogue) Load(tag string) ([]rulespec.Match, error) {
	if c == nil {
		return nil, fmt.Errorf("没有配置规则库地址")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	entry := c.entries[tag]
	if entry == nil {
		entry = c.readCache(tag)
	}
	if entry != nil && time.Since(entry.checked) < catalogueFresh {
		c.entries[tag] = entry
		return entry.rules, nil
	}
	rules, etag, body, err := c.fetch(tag, entry)
	if err != nil {
		if entry != nil {
			c.entries[tag] = entry
			return entry.rules, nil
		}
		return nil, err
	}
	c.entries[tag] = &catalogueEntry{etag: etag, rules: rules, checked: time.Now()}
	c.writeCache(tag, etag, body)
	return rules, nil
}

// cacheFile is where a tag's source and ETag are kept.
func (c *Catalogue) cacheFile(tag, ext string) string {
	return filepath.Join(c.dir, url.PathEscape(tag)+ext)
}

// readCache loads a rule set kept by an earlier run; checked is when it was
// last fetched or revalidated.
func (c *Catalogue) readCache(tag string) *catalogueEntry {
	if c.dir == "" {
		return nil
	}
	path := c.cacheFile(tag, ".json")
	info, err := os.Stat(path)
	if err != nil {
		return nil
	}
	body, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	rules, err := engine.ParseRuleSet(body)
	if err != nil {
		return nil
	}
	etag, _ := os.ReadFile(c.cacheFile(tag, ".etag"))
	return &catalogueEntry{etag: strings.TrimSpace(string(etag)), rules: rules, checked: info.ModTime()}
}

// writeCache keeps a fetched rule set (body nil: revalidated, only touched).
// Failures only cost the copy.
func (c *Catalogue) writeCache(tag, etag string, body []byte) {
	if c.dir == "" {
		return
	}
	path := c.cacheFile(tag, ".json")
	if body == nil {
		now := time.Now()
		_ = os.Chtimes(path, now, now)
		return
	}
	if err := os.MkdirAll(c.dir, 0o700); err != nil {
		return
	}
	if writeFileAtomic(path, body) == nil {
		_ = writeFileAtomic(c.cacheFile(tag, ".etag"), []byte(etag))
	}
}

// writeFileAtomic writes through a temporary file, so a crash never leaves
// half a rule set.
func writeFileAtomic(path string, data []byte) error {
	tmp, err := os.CreateTemp(filepath.Dir(path), "."+filepath.Base(path)+".*")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), path)
}

// fetch downloads a tag, or revalidates the cached copy (body nil then).
func (c *Catalogue) fetch(tag string, cached *catalogueEntry) (rules []rulespec.Match, etag string, body []byte, err error) {
	ctx, cancel := context.WithTimeout(context.Background(), catalogueTimeout)
	defer cancel()
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, c.base+url.PathEscape(tag)+".json", nil)
	if err != nil {
		return nil, "", nil, err
	}
	request.Header.Set("User-Agent", version.ClientUA)
	if cached != nil && cached.etag != "" {
		request.Header.Set("If-None-Match", cached.etag)
	}
	response, err := c.client.Do(request)
	if err != nil {
		return nil, "", nil, err
	}
	defer response.Body.Close()
	switch {
	case response.StatusCode == http.StatusNotModified && cached != nil:
		return cached.rules, cached.etag, nil, nil
	case response.StatusCode == http.StatusNotFound:
		return nil, "", nil, fmt.Errorf("规则库里没有")
	case response.StatusCode != http.StatusOK:
		return nil, "", nil, fmt.Errorf("规则库返回 %s", response.Status)
	}
	body, err = io.ReadAll(io.LimitReader(response.Body, 32<<20))
	if err != nil {
		return nil, "", nil, err
	}
	rules, err = engine.ParseRuleSet(body)
	if err != nil {
		return nil, "", nil, err
	}
	return rules, response.Header.Get("ETag"), body, nil
}
