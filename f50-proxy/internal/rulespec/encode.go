package rulespec

import "encoding/json"

// matchJSON is a match in sing-box's JSON shape; lists are always arrays.
type matchJSON struct {
	Type          string   `json:"type,omitempty"`
	Mode          string   `json:"mode,omitempty"`
	Rules         []Match  `json:"rules,omitempty"`
	Domain        []string `json:"domain,omitempty"`
	DomainSuffix  []string `json:"domain_suffix,omitempty"`
	DomainKeyword []string `json:"domain_keyword,omitempty"`
	DomainRegex   []string `json:"domain_regex,omitempty"`
	IPCIDR        []string `json:"ip_cidr,omitempty"`
	IPIsPrivate   bool     `json:"ip_is_private,omitempty"`
	Port          []uint16 `json:"port,omitempty"`
	PortRange     []string `json:"port_range,omitempty"`
	Network       []string `json:"network,omitempty"`
	Protocol      []string `json:"protocol,omitempty"`
	RuleSet       []string `json:"rule_set,omitempty"`
	Invert        bool     `json:"invert,omitempty"`
}

func (m Match) wire() matchJSON {
	return matchJSON{
		Type: m.Type, Mode: m.Mode, Rules: m.Rules,
		Domain: m.Domain, DomainSuffix: m.DomainSuffix, DomainKeyword: m.DomainKeyword, DomainRegex: m.DomainRegex,
		IPCIDR: m.IPCIDR, IPIsPrivate: m.IPIsPrivate, Port: m.Port, PortRange: m.PortRange,
		Network: m.Network, Protocol: m.Protocol, RuleSet: m.RuleSet, Invert: m.Invert,
	}
}

// MarshalJSON writes m as a sing-box rule without an action.
func (m Match) MarshalJSON() ([]byte, error) {
	return json.Marshal(m.wire())
}

// MarshalJSON writes r as a sing-box route rule.
func (r Rule) MarshalJSON() ([]byte, error) {
	return json.Marshal(struct {
		matchJSON
		Action   string `json:"action"`
		Outbound string `json:"outbound,omitempty"`
	}{r.Match.wire(), r.Action, r.Outbound})
}

// Map returns m as a decoded sing-box JSON object, for editing a profile
// document.
func (m Match) Map() map[string]any {
	data, err := m.MarshalJSON()
	if err != nil {
		panic(err)
	}
	result := map[string]any{}
	if err = json.Unmarshal(data, &result); err != nil {
		panic(err)
	}
	return result
}
