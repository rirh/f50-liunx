package rulespec

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"maps"
	"net/netip"
	"regexp"
	"slices"
	"strconv"
	"strings"
)

// Error reports an invalid rule. Path locates the field inside the rule,
// e.g. "rules[1].domain"; it is empty for the rule itself.
type Error struct {
	Path    string
	Message string
}

func (e *Error) Error() string {
	if e.Path == "" {
		return e.Message
	}
	return e.Path + "：" + e.Message
}

func errorAt(path, format string, args ...any) error {
	return &Error{Path: path, Message: fmt.Sprintf(format, args...)}
}

// UnmarshalJSON decodes a sing-box route rule in One Node's subset. Action
// defaults to route, as in sing-box.
func (r *Rule) UnmarshalJSON(data []byte) error {
	fields, err := decodeObject(data, "")
	if err != nil {
		return err
	}
	var rule Rule
	if raw, ok := fields["action"]; ok {
		delete(fields, "action")
		if rule.Action, err = decodeString(raw, "action"); err != nil {
			return err
		}
	}
	if raw, ok := fields["outbound"]; ok {
		delete(fields, "outbound")
		if rule.Outbound, err = decodeString(raw, "outbound"); err != nil {
			return err
		}
		rule.Outbound = strings.TrimSpace(rule.Outbound)
	}
	if err = rule.Match.decode(fields, "", 1); err != nil {
		return err
	}
	switch strings.TrimSpace(rule.Action) {
	case "", ActionRoute:
		rule.Action = ActionRoute
		if rule.Outbound == "" {
			return errorAt("outbound", "route 规则需要指定 outbound")
		}
	case ActionReject:
		rule.Action = ActionReject
		if rule.Outbound != "" {
			return errorAt("outbound", "reject 规则不能指定 outbound")
		}
	default:
		return errorAt("action", "只支持 route 和 reject")
	}
	*r = rule
	return nil
}

// UnmarshalJSON decodes a rule condition, which carries no action.
func (m *Match) UnmarshalJSON(data []byte) error {
	fields, err := decodeObject(data, "")
	if err != nil {
		return err
	}
	var match Match
	if err = match.decode(fields, "", 1); err != nil {
		return err
	}
	*m = match
	return nil
}

func (m *Match) decode(fields map[string]json.RawMessage, path string, depth int) error {
	if raw, ok := fields["type"]; ok {
		at := join(path, "type")
		typ, err := decodeString(raw, at)
		if err != nil {
			return err
		}
		switch strings.TrimSpace(typ) {
		case "", "default":
		case TypeLogical:
			m.Type = TypeLogical
		default:
			return errorAt(at, "只支持 default 和 logical")
		}
	}
	for _, key := range slices.Sorted(maps.Keys(fields)) {
		if err := m.decodeField(key, fields[key], path, depth); err != nil {
			return err
		}
	}
	if m.IsLogical() {
		if m.Mode == "" {
			return errorAt(join(path, "mode"), "逻辑规则需要 mode：and 或 or")
		}
		if len(m.Rules) == 0 {
			return errorAt(join(path, "rules"), "逻辑规则至少需要一条子规则")
		}
	} else if m.empty() {
		return errorAt(path, "至少需要一个匹配条件")
	}
	if depth == 1 && m.valueCount() > MaxValues {
		return errorAt(path, "匹配值最多 %d 个", MaxValues)
	}
	return nil
}

func (m *Match) decodeField(key string, raw json.RawMessage, path string, depth int) error {
	at := join(path, key)
	switch key {
	case "type":
		return nil
	case "invert":
		return decodeBool(raw, at, &m.Invert)
	}
	if m.IsLogical() {
		switch key {
		case "mode":
			mode, err := decodeString(raw, at)
			if err != nil {
				return err
			}
			mode = strings.ToLower(strings.TrimSpace(mode))
			if mode != ModeAnd && mode != ModeOr {
				return errorAt(at, "只支持 and 和 or")
			}
			m.Mode = mode
			return nil
		case "rules":
			return m.decodeRules(raw, at, depth)
		}
		return unknownField(at, key, true)
	}
	var err error
	switch key {
	case "domain":
		m.Domain, err = decodeList(raw, at, func(value string) (string, error) { return normalizeDomain(value, false) })
	case "domain_suffix":
		m.DomainSuffix, err = decodeList(raw, at, func(value string) (string, error) { return normalizeDomain(value, true) })
	case "domain_keyword":
		m.DomainKeyword, err = decodeList(raw, at, normalizeKeyword)
	case "domain_regex":
		m.DomainRegex, err = decodeList(raw, at, normalizeRegex)
	case "ip_cidr":
		m.IPCIDR, err = decodeList(raw, at, normalizeCIDR)
	case "ip_is_private":
		err = decodeBool(raw, at, &m.IPIsPrivate)
	case "port":
		m.Port, err = decodePorts(raw, at)
	case "port_range":
		m.PortRange, err = decodeList(raw, at, normalizePortRange)
	case "network":
		m.Network, err = decodeList(raw, at, oneOf(Networks))
	case "protocol":
		m.Protocol, err = decodeList(raw, at, oneOf(Protocols))
	case "rule_set":
		m.RuleSet, err = decodeList(raw, at, normalizeRuleSetTag)
	default:
		return unknownField(at, key, false)
	}
	return err
}

func (m *Match) decodeRules(raw json.RawMessage, at string, depth int) error {
	if depth >= MaxDepth {
		return errorAt(at, "逻辑规则最多嵌套 %d 层", MaxDepth)
	}
	var items []json.RawMessage
	if err := json.Unmarshal(raw, &items); err != nil {
		return errorAt(at, "必须是规则数组")
	}
	m.Rules = make([]Match, 0, len(items))
	for i, item := range items {
		itemPath := fmt.Sprintf("%s[%d]", at, i)
		fields, err := decodeObject(item, itemPath)
		if err != nil {
			return err
		}
		for _, key := range []string{"action", "outbound"} {
			if _, ok := fields[key]; ok {
				return errorAt(join(itemPath, key), "子规则不能带动作，动作只写在最外层")
			}
		}
		var nested Match
		if err = nested.decode(fields, itemPath, depth+1); err != nil {
			return err
		}
		m.Rules = append(m.Rules, nested)
	}
	return nil
}

var defaultFields = []string{
	"domain", "domain_suffix", "domain_keyword", "domain_regex", "ip_cidr", "ip_is_private",
	"port", "port_range", "network", "protocol", "rule_set",
}

// singBoxOnlyFields are sing-box rule fields outside the subset, reported as
// unsupported rather than unknown.
var singBoxOnlyFields = []string{
	"inbound", "ip_version", "auth_user", "client", "geosite", "geoip",
	"source_geoip", "source_ip_cidr", "source_ip_is_private", "source_port", "source_port_range",
	"source_mac_address", "source_hostname", "process_name", "process_path", "process_path_regex",
	"package_name", "package_name_regex", "user", "user_id", "clash_mode",
	"network_type", "network_is_expensive", "network_is_constrained", "interface_address",
	"network_interface_address", "default_interface_address", "wifi_ssid", "wifi_bssid",
	"preferred_by", "rule_set_ip_cidr_match_source", "rule_set_ip_cidr_accept_empty",
	"query_type", "method", "no_drop", "override_address", "override_port",
	"network_strategy", "fallback_delay", "udp_disable_domain_unmapping", "udp_connect",
	"udp_timeout", "tls_fragment", "tls_record_fragment", "sniffer", "timeout", "strategy", "server",
}

func unknownField(at, key string, logical bool) error {
	switch {
	case logical && slices.Contains(defaultFields, key):
		return errorAt(at, "逻辑规则不能直接写匹配条件，请放进 rules 的子规则里")
	case !logical && (key == "mode" || key == "rules"):
		return errorAt(at, "需要 \"type\": \"logical\" 才能使用 %s", key)
	case slices.Contains(singBoxOnlyFields, key):
		return errorAt(at, "暂不支持 sing-box 字段 %s", key)
	}
	return errorAt(at, "未知字段 %s", key)
}

func join(path, key string) string {
	if path == "" {
		return key
	}
	return path + "." + key
}

func decodeObject(data []byte, at string) (map[string]json.RawMessage, error) {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil || fields == nil {
		return nil, errorAt(at, "必须是 JSON 对象")
	}
	return fields, nil
}

func decodeString(raw json.RawMessage, at string) (string, error) {
	var value string
	if err := json.Unmarshal(raw, &value); err != nil {
		return "", errorAt(at, "必须是字符串")
	}
	return value, nil
}

func decodeBool(raw json.RawMessage, at string, target *bool) error {
	if err := json.Unmarshal(raw, target); err != nil {
		return errorAt(at, "必须是 true 或 false")
	}
	return nil
}

// decodeList reads a sing-box listable string field (one string or an
// array), normalizing each value and dropping blanks and duplicates.
func decodeList(raw json.RawMessage, at string, normalize func(string) (string, error)) ([]string, error) {
	var values []string
	if trimmed := bytes.TrimSpace(raw); len(trimmed) > 0 && trimmed[0] == '"' {
		var value string
		if err := json.Unmarshal(trimmed, &value); err != nil {
			return nil, errorAt(at, "必须是字符串或字符串数组")
		}
		values = []string{value}
	} else if err := json.Unmarshal(trimmed, &values); err != nil {
		return nil, errorAt(at, "必须是字符串或字符串数组")
	}
	result := make([]string, 0, len(values))
	seen := map[string]bool{}
	for _, raw := range values {
		value, err := normalize(raw)
		if err != nil {
			return nil, errorAt(at, "「%s」%s", strings.TrimSpace(raw), err.Error())
		}
		if value != "" && !seen[value] {
			seen[value] = true
			result = append(result, value)
		}
	}
	return result, nil
}

func decodePorts(raw json.RawMessage, at string) ([]uint16, error) {
	trimmed := bytes.TrimSpace(raw)
	if len(trimmed) > 0 && trimmed[0] != '[' && !bytes.Equal(trimmed, []byte("null")) {
		trimmed = slices.Concat([]byte("["), trimmed, []byte("]"))
	}
	var values []int
	if err := json.Unmarshal(trimmed, &values); err != nil {
		return nil, errorAt(at, "必须是端口号或端口号数组")
	}
	result := make([]uint16, 0, len(values))
	for _, value := range values {
		if value < 1 || value > 65535 {
			return nil, errorAt(at, "「%d」不是 1–65535 之间的端口", value)
		}
		if port := uint16(value); !slices.Contains(result, port) {
			result = append(result, port)
		}
	}
	return result, nil
}

var (
	hostnamePattern = regexp.MustCompile(`^[a-z0-9_]([a-z0-9_-]{0,61}[a-z0-9_])?(\.[a-z0-9_]([a-z0-9_-]{0,61}[a-z0-9_])?)*$`)
	keywordPattern  = regexp.MustCompile(`^[a-z0-9_.-]+$`)
	ruleSetPattern  = regexp.MustCompile(`^[a-z0-9][a-z0-9_.!@-]{0,63}$`)
)

// normalizeDomain lower-cases a domain and drops its trailing dot. A
// leading dot is kept for suffixes, where it means "subdomains only".
func normalizeDomain(raw string, suffix bool) (string, error) {
	value := strings.TrimSuffix(strings.ToLower(strings.TrimSpace(raw)), ".")
	if value == "" {
		return "", nil
	}
	prefix := ""
	if suffix && strings.HasPrefix(value, ".") {
		prefix, value = ".", value[1:]
	}
	switch {
	case strings.Contains(value, "*"):
		return "", errors.New("不支持通配符；要匹配一个域名及其子域名，请用域名后缀")
	case strings.ContainsFunc(value, func(r rune) bool { return r > 127 }):
		return "", errors.New("请填写 Punycode 形式（xn--…）")
	case len(value) > 253 || !hostnamePattern.MatchString(value):
		return "", errors.New("不是有效的域名")
	}
	return prefix + value, nil
}

func normalizeKeyword(raw string) (string, error) {
	value := strings.ToLower(strings.TrimSpace(raw))
	if value != "" && (len(value) > 253 || !keywordPattern.MatchString(value)) {
		return "", errors.New("只能包含字母、数字、点、横线和下划线")
	}
	return value, nil
}

func normalizeRegex(raw string) (string, error) {
	value := strings.TrimSpace(raw)
	if len(value) > MaxRegexLength {
		return "", fmt.Errorf("超过 %d 个字符", MaxRegexLength)
	}
	if _, err := regexp.Compile(value); err != nil {
		return "", errors.New("不是有效的正则表达式")
	}
	return value, nil
}

func normalizeCIDR(raw string) (string, error) {
	value := strings.TrimSpace(raw)
	if value == "" {
		return "", nil
	}
	if prefix, err := netip.ParsePrefix(value); err == nil {
		return prefix.Masked().String(), nil
	}
	address, err := netip.ParseAddr(value)
	if err != nil {
		return "", errors.New("不是有效的 IP 或 CIDR")
	}
	return netip.PrefixFrom(address, address.BitLen()).String(), nil
}

// normalizePortRange accepts sing-box ranges: "first:last", ":last" or "first:".
func normalizePortRange(raw string) (string, error) {
	value := strings.TrimSpace(raw)
	if value == "" {
		return "", nil
	}
	first, last, found := strings.Cut(value, ":")
	if !found || (first == "" && last == "") {
		return "", errors.New("格式为 起始:结束，例如 1000:2000、:3000、4000:")
	}
	parse := func(text string) (uint64, error) {
		if text == "" {
			return 0, nil
		}
		port, err := strconv.ParseUint(text, 10, 16)
		if err != nil || port == 0 {
			return 0, errors.New("端口必须是 1–65535")
		}
		return port, nil
	}
	start, err := parse(first)
	if err != nil {
		return "", err
	}
	end, err := parse(last)
	if err != nil {
		return "", err
	}
	if first != "" && last != "" && start > end {
		return "", errors.New("起始端口不能大于结束端口")
	}
	return first + ":" + last, nil
}

func oneOf(allowed []string) func(string) (string, error) {
	return func(raw string) (string, error) {
		value := strings.ToLower(strings.TrimSpace(raw))
		if value != "" && !slices.Contains(allowed, value) {
			return "", fmt.Errorf("只支持 %s", strings.Join(allowed, "、"))
		}
		return value, nil
	}
}

func normalizeRuleSetTag(raw string) (string, error) {
	value := strings.ToLower(strings.TrimSpace(raw))
	if value != "" && !ruleSetPattern.MatchString(value) {
		return "", errors.New("不是有效的规则集名称")
	}
	return value, nil
}
