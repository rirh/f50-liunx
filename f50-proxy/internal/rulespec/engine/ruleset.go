package engine

import (
	"bytes"
	"encoding/json"
	"fmt"

	"f50/proxy/internal/rulespec"
)

// ParseRuleSet reads a sing-box source rule set ({"version", "rules"}) of
// address rules, as the rule catalogue serves at <tag>.json. Unlike route
// rules a rule set takes any number of values.
func ParseRuleSet(data []byte) ([]rulespec.Match, error) {
	var source struct {
		Version int               `json:"version"`
		Rules   []json.RawMessage `json:"rules"`
	}
	if err := json.Unmarshal(data, &source); err != nil {
		return nil, fmt.Errorf("rule set: %w", err)
	}
	if source.Version < 1 || len(source.Rules) == 0 {
		return nil, fmt.Errorf("rule set: version %d with %d rules", source.Version, len(source.Rules))
	}
	rules := make([]rulespec.Match, 0, len(source.Rules))
	for i, raw := range source.Rules {
		var rule struct {
			Domain        []string `json:"domain"`
			DomainSuffix  []string `json:"domain_suffix"`
			DomainKeyword []string `json:"domain_keyword"`
			DomainRegex   []string `json:"domain_regex"`
			IPCIDR        []string `json:"ip_cidr"`
			Invert        bool     `json:"invert"`
		}
		decoder := json.NewDecoder(bytes.NewReader(raw))
		decoder.DisallowUnknownFields()
		if err := decoder.Decode(&rule); err != nil {
			return nil, fmt.Errorf("rule set rule %d: %w", i, err)
		}
		match := rulespec.Match{
			Domain: rule.Domain, DomainSuffix: rule.DomainSuffix, DomainKeyword: rule.DomainKeyword,
			DomainRegex: rule.DomainRegex, IPCIDR: rule.IPCIDR, Invert: rule.Invert,
		}
		rules = append(rules, match)
	}
	return rules, nil
}
