// Package route compiles the 配置 tab's sing-box route document into rule
// engine steps and 策略组, and builds the Policy each routing mode runs.
package route

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"slices"
	"strings"

	"f50/proxy/internal/geodata"
	"f50/proxy/internal/rulespec"
	"f50/proxy/internal/rulespec/engine"
)

// Route is the 配置 tab's rules, ready for the core's router. The source is
// sing-box route JSON, the format the One Node backend and most configs use:
// a whole config (only "route" is read), a route object, or a bare rules
// array. Rules keep sing-box's semantics (pkg/rulespec, run by the rule
// engine); in 规则 mode they run after the private ranges and before the
// mainland lists, and Final takes what nothing matched.
//
// What One Node's rule language lacks (wifi_ssid, clash_mode, process_name …)
// is skipped with a warning; malformed values are errors.
//
// On top of sing-box's rules a document may define 策略组 (groups) and 应用分流
// (apps, rules the web writes from its catalogue; they run after rules). An
// outbound is direct, proxy (the node the user picked), another node or a group.
// A rule whose node or group is missing or empty is skipped with a warning.
// Documents without groups or apps keep the old reading: an unknown outbound
// is the picked node.
type Route struct {
	Steps []engine.Step `json:"-"`
	// Sets holds the rule sets the steps reference, loaded once.
	Sets  map[string][]rulespec.Match `json:"-"`
	Final string                      `json:"final"` // proxy | direct | block | a node or group name
	// Groups are the groups with members, nodes resolved; Nodes the other nodes
	// the route needs an outbound for (rules' and groups' together).
	Groups   []Group  `json:"groups"`
	Nodes    []string `json:"-"`
	Source   int      `json:"source"` // sing-box rules that took effect (apps included)
	Count    int      `json:"rules"`  // the same, kept for the UI
	Warnings []string `json:"warnings"`

	finalTag string // Final as the router's tag (NodeTag, GroupTag); empty: Final
}

// FinalTag is Final as the router's tag (NodeTag, GroupTag); empty when
// Final is direct, block or proxy.
func (r Route) FinalTag() string { return r.finalTag }

// Group is one 策略组: select (the user picks), urltest (lowest delay) or
// fallback (the first that answers), over nodes in order.
type Group struct {
	Name     string   `json:"name"`
	Type     string   `json:"type"`
	Nodes    []string `json:"nodes"`
	Interval int      `json:"interval,omitempty"` // seconds between tests (urltest, fallback)
}

const (
	GroupSelect   = "select"
	GroupURLTest  = "urltest"
	GroupFallback = "fallback"

	defaultGroupInterval = 300
	minGroupInterval     = 60
)

// Rules name nodes and groups by these tags; the router turns a group into
// its current node, and each node used gets an Xray outbound of its tag.
const (
	nodeTagPrefix  = "node:"
	groupTagPrefix = "group:"
)

func NodeTag(name string) string { return nodeTagPrefix + name }

func GroupTag(name string) string { return groupTagPrefix + name }

const maxRouteRules = 5000

// actionOptions are sing-box rule fields that tune the action, not the match.
var actionOptions = []string{
	"action", "outbound", "method", "no_drop", "override_address", "override_port", "network_strategy",
	"fallback_delay", "udp_disable_domain_unmapping", "udp_connect", "udp_timeout", "tls_fragment",
	"tls_record_fragment", "sniffer", "timeout", "strategy", "server",
}

type compiler struct {
	route   *Route
	where   string                       // "第 3 条" for messages
	inline  map[string][]json.RawMessage // inline rule_set tag → rules
	remote  *Catalogue                   // the One Node rule catalogue, or nil
	asProxy []string                     // outbound names sent to the current node

	nodes  map[string]bool   // node names the request lists
	groups map[string]*Group // usable groups by name
	empty  map[string]bool   // groups defined without a usable member
	strict bool              // groups or apps present: unknown outbounds skip the rule
	used   map[string]bool   // nodes already in route.Nodes
}

func (c *compiler) warn(format string, args ...any) {
	c.route.Warnings = append(c.route.Warnings, c.where+"："+fmt.Sprintf(format, args...))
}

func (c *compiler) fail(format string, args ...any) error {
	return errors.New(c.where + "：" + fmt.Sprintf(format, args...))
}

// CompileRoute parses the 配置 tab's JSON; rule sets it neither defines inline
// nor finds built in come from remote (the rule catalogue, may be nil). nodes
// are the nodes outbounds and groups may name, in the order filters list them.
func CompileRoute(text string, remote *Catalogue, nodes ...string) (Route, error) {
	route := Route{Final: "proxy", Sets: map[string][]rulespec.Match{}, Groups: []Group{}, Warnings: []string{}}
	if strings.TrimSpace(text) == "" {
		return route, nil
	}
	var src struct {
		Groups  []groupSource     `json:"groups"`
		Apps    []json.RawMessage `json:"apps"`
		Rules   []json.RawMessage `json:"rules"`
		RuleSet []json.RawMessage `json:"rule_set"`
		Final   string            `json:"final"`
	}
	raw := bytes.TrimSpace([]byte(text))
	if err := decodeJSON(raw, func(b []byte) error {
		if b[0] == '[' {
			return json.Unmarshal(b, &src.Rules)
		}
		var whole struct {
			Route *json.RawMessage `json:"route"`
		}
		if err := json.Unmarshal(b, &whole); err == nil && whole.Route != nil {
			b = *whole.Route
		}
		return json.Unmarshal(b, &src)
	}); err != nil {
		return route, err
	}

	c := &compiler{
		route: &route, inline: map[string][]json.RawMessage{}, remote: remote,
		nodes: map[string]bool{}, groups: map[string]*Group{}, empty: map[string]bool{}, used: map[string]bool{},
		strict: src.Groups != nil || src.Apps != nil,
	}
	for _, name := range nodes {
		c.nodes[name] = true
	}
	if err := c.compileGroups(src.Groups, nodes); err != nil {
		return route, err
	}
	for i, raw := range src.RuleSet {
		var set struct {
			Tag   string            `json:"tag"`
			Type  string            `json:"type"`
			Rules []json.RawMessage `json:"rules"`
		}
		c.where = fmt.Sprintf("rule_set 第 %d 个", i+1)
		if err := json.Unmarshal(raw, &set); err != nil || set.Tag == "" {
			return route, c.fail("需要 tag")
		}
		// Remote sets (.srs URLs) load by tag from the rule catalogue instead.
		if set.Type == "inline" || (set.Type == "" && set.Rules != nil) {
			c.inline[set.Tag] = set.Rules
		}
	}
	if src.Final != "" {
		c.where = "final"
		if tag, ok := c.outbound(src.Final); ok {
			route.Final, route.finalTag = src.Final, tag
			if tag == "direct" || tag == "block" || tag == "proxy" {
				route.Final, route.finalTag = tag, ""
			}
		} else {
			c.warn("出站 %s %s，改用当前选中的节点", src.Final, c.missing(src.Final))
		}
	}
	for i, raw := range src.Rules {
		c.where = fmt.Sprintf("第 %d 条", i+1)
		if err := c.rule(raw); err != nil {
			return route, err
		}
	}
	for i, raw := range src.Apps {
		c.where = fmt.Sprintf("应用第 %d 个", i+1)
		if err := c.app(raw); err != nil {
			return route, err
		}
	}
	if len(c.asProxy) > 0 {
		route.Warnings = append(route.Warnings, fmt.Sprintf(
			"出站 %s 都按当前选中的节点处理", strings.Join(c.asProxy, "、")))
	}
	route.Count = route.Source
	if route.Count > maxRouteRules {
		return route, fmt.Errorf("有 %d 条规则，最多 %d 条", route.Count, maxRouteRules)
	}
	// Compile once so a bad pattern fails here, not when connecting.
	if _, err := engine.New(route.Steps, route.ruleSets); err != nil {
		return route, err
	}
	return route, nil
}

// decodeJSON runs decode and turns syntax errors into line and column.
func decodeJSON(raw []byte, decode func([]byte) error) error {
	err := decode(raw)
	var syntax *json.SyntaxError
	var typ *json.UnmarshalTypeError
	switch {
	case err == nil:
		return nil
	case errors.As(err, &syntax):
		line, col := 1, 1
		for _, b := range raw[:min(int(syntax.Offset), len(raw))] {
			if b == '\n' {
				line, col = line+1, 1
			} else {
				col++
			}
		}
		return fmt.Errorf("JSON 第 %d 行第 %d 列有语法错误：%v", line, col, syntax)
	case errors.As(err, &typ):
		return fmt.Errorf("JSON 字段 %s 的类型不对（需要 %s）", typ.Field, typ.Type)
	default:
		return fmt.Errorf("JSON 格式不对：%v", err)
	}
}

// outbound maps an outbound name to the router's tag: direct, block, proxy, a
// group or a named node. Other names are the node the user picked in documents
// from before groups (sing-box configs name their own outbounds); with groups
// or apps they are missing, and ok is false.
func (c *compiler) outbound(name string) (tag string, ok bool) {
	switch name {
	case "direct", "block", "proxy":
		return name, true
	}
	if c.groups[name] != nil {
		return GroupTag(name), true
	}
	if c.nodes[name] {
		c.use(name)
		return NodeTag(name), true
	}
	if c.strict || c.empty[name] {
		return "", false
	}
	if !slices.Contains(c.asProxy, name) {
		c.asProxy = append(c.asProxy, name)
	}
	return "proxy", true
}

// missing says why outbound refused a name.
func (c *compiler) missing(name string) string {
	if c.empty[name] {
		return "这个策略组没有可用节点"
	}
	return "不是节点或策略组"
}

func (c *compiler) use(node string) {
	if !c.used[node] {
		c.used[node] = true
		c.route.Nodes = append(c.route.Nodes, node)
	}
}

// app is one 应用分流 card: its match (a default rule, written by the web's
// Catalogue) sent to its outbound.
func (c *compiler) app(raw json.RawMessage) error {
	var app struct {
		App      string                     `json:"app"`
		Outbound string                     `json:"outbound"`
		Match    map[string]json.RawMessage `json:"match"`
	}
	if err := json.Unmarshal(raw, &app); err != nil {
		return c.fail("应用必须是对象：%v", err)
	}
	if app.App != "" {
		c.where = "应用 " + app.App
	}
	if app.Outbound == "" {
		return c.fail("缺少 outbound")
	}
	if len(app.Match) == 0 {
		return c.fail("缺少 match")
	}
	if _, ok := app.Match["action"]; ok {
		return c.fail("match 不能带 action")
	}
	if _, ok := app.Match["outbound"]; ok {
		return c.fail("match 不能带 outbound")
	}
	app.Match["outbound"], _ = json.Marshal(app.Outbound)
	rule, err := json.Marshal(app.Match)
	if err != nil {
		return c.fail("%v", err)
	}
	return c.rule(rule)
}

func (c *compiler) rule(raw json.RawMessage) error {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil || fields == nil {
		return c.fail("规则必须是对象")
	}
	action, _ := stringField(fields, "action")
	rule := rulespec.Rule{Action: rulespec.ActionRoute}
	switch action {
	case "", "route":
		out, _ := stringField(fields, "outbound")
		if out == "" {
			return c.fail("缺少 outbound")
		}
		tag, ok := c.outbound(out)
		if !ok {
			c.warn("出站 %s %s，已跳过", out, c.missing(out))
			return nil
		}
		rule.Outbound = tag
	case "reject":
		rule.Action = rulespec.ActionReject
	case "sniff", "hijack-dns":
		return nil // the core always sniffs; DNS is not routed by rules here
	case "resolve":
		c.route.Steps = append(c.route.Steps, engine.Step{Resolve: true})
		return nil
	default:
		c.warn("action %s 不支持，已跳过", action)
		return nil
	}
	for _, key := range actionOptions {
		delete(fields, key)
	}
	matchJSON, err := json.Marshal(fields)
	if err != nil {
		return c.fail("%v", err)
	}
	if err = json.Unmarshal(matchJSON, &rule.Match); err != nil {
		var spec *rulespec.Error
		if errors.As(err, &spec) && unsupported(spec) {
			c.warn("%s，已跳过", spec.Error())
			return nil
		}
		return c.fail("%v", err)
	}
	for _, tag := range rule.RuleSets() {
		if err := c.loadSet(tag); err != nil {
			c.warn("规则集 %s %v，已跳过", tag, err)
			return nil
		}
	}
	c.route.Steps = append(c.route.Steps, engine.Step{Rule: rule})
	c.route.Source++
	return nil
}

// unsupported tells fields outside One Node's subset from malformed values.
func unsupported(err *rulespec.Error) bool {
	return strings.HasPrefix(err.Message, "暂不支持 sing-box 字段") || strings.HasPrefix(err.Message, "未知字段")
}

// loadSet finds a rule set: defined inline, built in, or in the catalogue.
func (c *compiler) loadSet(tag string) error {
	if _, ok := c.route.Sets[tag]; ok {
		return nil
	}
	if raws, ok := c.inline[tag]; ok {
		rules := make([]rulespec.Match, 0, len(raws))
		for _, raw := range raws {
			var match rulespec.Match
			if err := json.Unmarshal(raw, &match); err != nil {
				return fmt.Errorf("里的规则无效：%v", err)
			}
			rules = append(rules, match)
		}
		c.route.Sets[tag] = rules
		return nil
	}
	if rules, ok := geodata.Builtin(tag); ok {
		c.route.Sets[tag] = rules
		return nil
	}
	rules, err := c.remote.Load(tag)
	if err != nil {
		return fmt.Errorf("无法从规则库获取（%v）", err)
	}
	c.route.Sets[tag] = rules
	return nil
}

func stringField(f map[string]json.RawMessage, key string) (string, bool) {
	var s string
	if raw, ok := f[key]; ok && json.Unmarshal(raw, &s) == nil {
		return s, true
	}
	return "", false
}
