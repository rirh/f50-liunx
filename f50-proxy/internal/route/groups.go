package route

import (
	"fmt"
	"regexp"
	"strings"
)

func CutGroupTag(tag string) (string, bool) {
	return strings.CutPrefix(tag, groupTagPrefix)
}

type groupSource struct {
	Name     string   `json:"name"`
	Type     string   `json:"type"`
	Nodes    []string `json:"nodes"`
	Filter   string   `json:"filter"`
	Interval int      `json:"interval"`
}

// compileGroups resolves each group's members: listed nodes first, then the
// nodes the filter matches, each once. Malformed groups are errors; members
// that are not nodes are skipped, and a group left empty makes the rules that
// use it skip.
func (c *compiler) compileGroups(sources []groupSource, nodes []string) error {
	defined := map[string]bool{}
	for _, src := range sources {
		defined[src.Name] = true
	}
	for i, src := range sources {
		c.where = fmt.Sprintf("策略组第 %d 个", i+1)
		name := strings.TrimSpace(src.Name)
		switch {
		case name == "":
			return c.fail("需要 name")
		case name != src.Name:
			return c.fail("组名 %q 前后不能有空格", src.Name)
		case name == "direct" || name == "proxy" || name == "block":
			return c.fail("组名不能叫 %s", name)
		case c.groups[name] != nil || c.empty[name]:
			return c.fail("组名 %s 重复", name)
		}
		c.where = "策略组 " + name
		switch src.Type {
		case GroupSelect, GroupURLTest, GroupFallback:
		default:
			return c.fail("type 需要是 select、urltest 或 fallback")
		}
		if c.nodes[name] {
			// Nodes come and go with the subscription, so this is not the text's fault alone.
			c.warn("和节点重名，已跳过（规则里的 %s 指节点）", name)
			continue
		}
		group := &Group{Name: name, Type: src.Type, Nodes: []string{}}
		if src.Type != GroupSelect {
			group.Interval = defaultGroupInterval
			if src.Interval != 0 {
				group.Interval = src.Interval
			}
			if group.Interval < minGroupInterval {
				c.warn("interval 最小 %d 秒，已按 %d 秒", minGroupInterval, minGroupInterval)
				group.Interval = minGroupInterval
			}
		}
		seen := map[string]bool{}
		add := func(node string) {
			if !seen[node] {
				seen[node] = true
				group.Nodes = append(group.Nodes, node)
			}
		}
		for _, node := range src.Nodes {
			switch {
			case c.nodes[node]:
				add(node)
			case defined[node]:
				c.warn("组里不能嵌套策略组 %s，已跳过", node)
			default:
				c.warn("节点 %s 不存在，已跳过", node)
			}
		}
		if src.Filter != "" {
			re, err := regexp.Compile(src.Filter)
			if err != nil {
				return c.fail("filter 不是有效的正则：%v", err)
			}
			for _, node := range nodes {
				if re.MatchString(node) {
					add(node)
				}
			}
		}
		if len(group.Nodes) == 0 {
			c.warn("没有可用节点，用到它的规则会跳过")
			c.empty[name] = true
			continue
		}
		// Every group runs (and can be tested and picked), used by a rule or not.
		for _, node := range group.Nodes {
			c.use(node)
		}
		c.groups[name] = group
		c.route.Groups = append(c.route.Groups, *group)
	}
	return nil
}
