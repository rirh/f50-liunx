// Command genrules builds the mainland-China lists that 规则 (rule) mode routes
// direct, from data already in the workspace (no downloads):
//
//   - internal/geodata/rules/cn-ip.txt: CN IPv4/IPv6 allocations from APNIC's delegated file,
//     merged into minimal CIDRs.
//   - internal/geodata/rules/cn-domain.txt: the sing-box cn-direct rule source (geosite
//     geolocation-cn + tencent + alibaba + bytedance) as Xray domain matchers,
//     minus the ByteDance services that do not serve the mainland.
//
// The engine embeds both files; rerun this to refresh them (both inputs live
// in the one-node workspace: one-ip/backend/data/intel/delegated-apnic.txt
// and backend/log/mobile-rules/cn-direct.json).
package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"net/netip"
	"os"
	"slices"
	"strconv"
	"strings"
	"time"
)

func main() {
	apnic := flag.String("apnic", "", "APNIC delegated-apnic file (required)")
	domains := flag.String("domains", "", "sing-box rule-set source with CN domains (required)")
	out := flag.String("out", "internal/geodata/rules", "output directory")
	flag.Parse()
	if *apnic == "" || *domains == "" {
		fmt.Fprintln(os.Stderr, "usage: genrules --apnic delegated-apnic.txt --domains cn-direct.json [--out rules]")
		os.Exit(2)
	}

	ips, err := cnPrefixes(*apnic)
	must(err)
	matchers, err := cnDomains(*domains)
	must(err)
	stamp := time.Now().Format("2006-01-02")
	must(write(*out+"/cn-ip.txt", fmt.Sprintf("# CN IP ranges from APNIC delegated data (%s), merged %s", *apnic, stamp), ips))
	must(write(*out+"/cn-domain.txt", fmt.Sprintf("# CN domains from %s (sing-geosite snapshot), converted %s", *domains, stamp), matchers))
	fmt.Printf("cn-ip.txt: %d prefixes, cn-domain.txt: %d matchers\n", len(ips), len(matchers))
}

// cnPrefixes reads "apnic|CN|ipv4|1.0.1.0|256|..." (count of addresses) and
// "apnic|CN|ipv6|2001:250::|35|..." (prefix length) lines.
func cnPrefixes(path string) ([]string, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	type span struct{ lo, hi netip.Addr }
	var v4 []span
	var v6 []netip.Prefix
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		p := strings.Split(sc.Text(), "|")
		if len(p) < 7 || p[1] != "CN" || (p[6] != "allocated" && p[6] != "assigned") {
			continue
		}
		start, err := netip.ParseAddr(p[3])
		if err != nil {
			continue
		}
		n, err := strconv.ParseUint(p[4], 10, 64)
		if err != nil || n == 0 {
			continue
		}
		switch p[2] {
		case "ipv4":
			end := u32(start) + uint32(n) - 1
			v4 = append(v4, span{start, addr4(end)})
		case "ipv6":
			v6 = append(v6, netip.PrefixFrom(start, int(n)).Masked())
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	if len(v4) == 0 {
		return nil, fmt.Errorf("no CN ipv4 lines in %s", path)
	}

	// IPv4: merge adjacent/overlapping ranges, then cut each into CIDRs.
	slices.SortFunc(v4, func(a, b span) int { return a.lo.Compare(b.lo) })
	merged := []span{v4[0]}
	for _, s := range v4[1:] {
		last := &merged[len(merged)-1]
		if u32(s.lo) <= u32(last.hi)+1 {
			if s.hi.Compare(last.hi) > 0 {
				last.hi = s.hi
			}
			continue
		}
		merged = append(merged, s)
	}
	var out []string
	for _, s := range merged {
		for lo, hi := uint64(u32(s.lo)), uint64(u32(s.hi)); lo <= hi; {
			bits := 32
			for bits > 0 {
				size := uint64(1) << (32 - (bits - 1))
				if lo%size != 0 || lo+size-1 > hi {
					break
				}
				bits--
			}
			out = append(out, netip.PrefixFrom(addr4(uint32(lo)), bits).String())
			lo += uint64(1) << (32 - bits)
		}
	}

	// IPv6: allocations are already prefixes; drop ones covered by a shorter one.
	slices.SortFunc(v6, func(a, b netip.Prefix) int {
		if c := a.Addr().Compare(b.Addr()); c != 0 {
			return c
		}
		return a.Bits() - b.Bits()
	})
	var keep []netip.Prefix
	for _, p := range v6 {
		if len(keep) > 0 && keep[len(keep)-1].Contains(p.Addr()) && keep[len(keep)-1].Bits() <= p.Bits() {
			continue
		}
		keep = append(keep, p)
	}
	for _, p := range keep {
		out = append(out, p.String())
	}
	return out, nil
}

// notMainland are what v2fly tags @!cn ("not serving for cn") in the lists its
// bytedance geosite includes: data/tiktok, data/bytedance-ai-!cn and data/dola
// (domain-list-community, 2026-10-30). The cn-direct source lost the tags, so
// TikTok, Coze, Trae, Cici… went direct, which those services refuse from the
// mainland. Plain entries cover subdomains; full: entries only themselves.
var notMainland = strings.Fields(`
	bytedapm.com cici.com ciciai.com ciciaicdn.com coze.com dola.com marscode.com trae.ai
	muscdn.com musical.ly tik-tokapi.com tiktok-minis.com tiktok-row.net tiktok.com
	tiktokcdn-eu.com tiktokcdn-us.com tiktokcdn-us.com.edgesuite.net tiktokcdn.com
	tiktokcdn.com.akamaized.net tiktokcdn.com.edgesuite.net tiktokd.net tiktokd.org
	tiktokeu-cdn.com tiktokglobalshopv.com tiktokminis.us tiktokrow-cdn.com tiktokshop.com
	tiktokstaticb.com tiktokv.com tiktokv.com.edgekey.net tiktokv.com.edgesuite.net
	tiktokv.eu tiktokv.us tiktokvapp.com tiktokw.eu tiktokw.us ttcdn-us.com ttlivecdn.com
	ttoverseaus.net ttwstatic.com
	full:p16-tiktok-dm-sticker-sign-sg.ibyteimg.com full:p16-tiktokcdn-com.akamaized.net
	full:roovza-launches.appsflyersdk.com full:roovza.inapps.appsflyersdk.com
	full:roovza.launches.appsflyersdk.com full:roovza.skadsdk.appsflyersdk.com
`)

// overseas reports whether a domain:/full: matcher falls under notMainland.
func overseas(matcher string) bool {
	_, name, ok := strings.Cut(matcher, ":")
	if !ok || strings.HasPrefix(matcher, "regexp:") {
		return false
	}
	for _, entry := range notMainland {
		if full, ok := strings.CutPrefix(entry, "full:"); ok {
			if name == full {
				return true
			}
		} else if name == entry || strings.HasSuffix(name, "."+entry) {
			return true
		}
	}
	return false
}

// cnDomains converts {"rules":[{"domain":[…],"domain_suffix":[…],"domain_regex":[…]}]}
// into Xray matchers: full:, domain: (suffix incl. subdomains), regexp:.
func cnDomains(path string) ([]string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var src struct {
		Rules []struct {
			Domain       []string `json:"domain"`
			DomainSuffix []string `json:"domain_suffix"`
			DomainRegex  []string `json:"domain_regex"`
		} `json:"rules"`
	}
	if err := json.Unmarshal(raw, &src); err != nil {
		return nil, err
	}
	seen := map[string]bool{}
	var out []string
	add := func(m string) {
		if !seen[m] && !overseas(m) {
			seen[m] = true
			out = append(out, m)
		}
	}
	// The geolocation-cn snapshot lists sites, not the .cn TLD itself, which
	// every common China ruleset sends direct.
	add("domain:cn")
	for _, r := range src.Rules {
		for _, d := range r.Domain {
			add("full:" + strings.ToLower(d))
		}
		for _, d := range r.DomainSuffix {
			add("domain:" + strings.ToLower(strings.TrimPrefix(d, ".")))
		}
		for _, d := range r.DomainRegex {
			add("regexp:" + d)
		}
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("no domains in %s", path)
	}
	return out, nil
}

func u32(a netip.Addr) uint32 {
	b := a.As4()
	return uint32(b[0])<<24 | uint32(b[1])<<16 | uint32(b[2])<<8 | uint32(b[3])
}

func addr4(v uint32) netip.Addr {
	return netip.AddrFrom4([4]byte{byte(v >> 24), byte(v >> 16), byte(v >> 8), byte(v)})
}

func write(path, header string, lines []string) error {
	return os.WriteFile(path, []byte(header+"\n"+strings.Join(lines, "\n")+"\n"), 0o644)
}

func must(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, "genrules:", err)
		os.Exit(1)
	}
}
