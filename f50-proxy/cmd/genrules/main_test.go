package main

import "testing"

func TestOverseasLeavesMainlandByteDance(t *testing.T) {
	for matcher, want := range map[string]bool{
		"domain:tiktok.com":                    true,
		"full:www.tiktok.com":                  true,
		"domain:tiktokv.com.edgesuite.net":     true,
		"full:p16-tiktokcdn-com.akamaized.net": true,
		"domain:trae.ai":                       true,
		"domain:douyin.com":                    false,
		"domain:trae.cn":                       false,
		"domain:akamaized.net":                 false,
		"full:p16-other.akamaized.net":         false,
		"domain:nottiktok.com":                 false,
		"regexp:^tiktok\\.com$":                false,
		"full:roovza.skadsdk.appsflyersdk.com": true,
		"full:other.skadsdk.appsflyersdk.com":  false,
	} {
		if got := overseas(matcher); got != want {
			t.Errorf("overseas(%q) = %v, want %v", matcher, got, want)
		}
	}
}
