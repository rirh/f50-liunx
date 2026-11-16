package geodata

import (
	"net/netip"
	"slices"
	"testing"
)

// 规则 mode sends mainland China direct and everything else through the node.
func TestCNRuleLists(t *testing.T) {
	has := func(list []string, item string) bool { return slices.Contains(list, item) }
	for _, d := range []string{"domain:baidu.com", "domain:qq.com", "domain:taobao.com", "domain:bilibili.com", "domain:cn"} {
		if !has(CNDomains(), d) {
			t.Errorf("CN domain list misses %s", d)
		}
	}
	for _, d := range []string{"domain:google.com", "domain:youtube.com", "domain:openai.com"} {
		if has(CNDomains(), d) {
			t.Errorf("CN domain list wrongly has %s", d)
		}
	}
	covered := func(ip string) bool {
		a := netip.MustParseAddr(ip)
		for _, c := range CNIPs() {
			if netip.MustParsePrefix(c).Contains(a) {
				return true
			}
		}
		return false
	}
	for _, ip := range []string{"223.5.5.5", "114.114.114.114", "119.29.29.29", "240e::1"} {
		if !covered(ip) {
			t.Errorf("CN IP list misses %s", ip)
		}
	}
	for _, ip := range []string{"8.8.8.8", "1.1.1.1", "69.63.210.90", "2001:4860:4860::8888"} {
		if covered(ip) {
			t.Errorf("CN IP list wrongly covers %s", ip)
		}
	}
}

// The bytedance geosite includes TikTok and ByteDance's overseas AI apps,
// which refuse mainland connections: they must reach the node, while the
// mainland versions stay direct (cmd/genrules notMainland).
func TestBuiltinListLeavesOverseasByteDanceToTheNode(t *testing.T) {
	list := map[string]bool{}
	for _, m := range CNDomains() {
		list[m] = true
	}
	for _, gone := range []string{"domain:tiktok.com", "domain:tiktokv.com", "domain:musical.ly", "domain:coze.com", "domain:trae.ai", "full:p16-tiktokcdn-com.akamaized.net"} {
		if list[gone] {
			t.Errorf("%s is overseas but routed direct", gone)
		}
	}
	for _, kept := range []string{"domain:douyin.com", "domain:bytedance.com", "domain:coze.cn", "domain:trae.cn", "domain:feishu.cn"} {
		if !list[kept] {
			t.Errorf("%s is mainland and must stay direct", kept)
		}
	}
}
