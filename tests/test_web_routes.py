"""The web console's pages at the site root (history routing): uci-defaults 93-f50-web points uhttpd's error_page at the
console, f50d/openwrt/install.sh makes the front page a link to it, f50-web builds its files for /f50/ and routes at /."""
import os
import re
import unittest

from helpers import TOP, ShellTest

SCRIPT = TOP / 'systems' / 'openwrt' / 'overlay' / 'etc' / 'uci-defaults' / '93-f50-web'


class ErrorPage(ShellTest):
    def setUp(self):
        super().setUp()
        self.www = self.tmp / 'www'
        (self.www / 'f50').mkdir(parents=True)
        (self.www / 'f50' / 'index.html').write_text('<!doctype html>')
        self.conf = self.tmp / 'uci-uhttpd'
        self.conf.write_text("uhttpd.main=uhttpd\nuhttpd.main.home='/www'\n")
        # uci: get a value from the file, set appends one (the last line of a key wins), commit is logged
        self.stub('uci', '[ "$1" = -q ] && shift\n'
                         'case $1 in\n'
                         '    get) v=$(grep "^$2=" "$STUBLOG/uci-uhttpd" | tail -n 1) || exit 1; [ -n "$v" ] || exit 1; '
                         'v=${v#*=}; v=${v#\\\'}; echo "${v%\\\'}" ;;\n'
                         '    set) echo "$2" >> "$STUBLOG/uci-uhttpd" ;;\n'
                         '    commit) echo "$2" >> "$STUBLOG/commit.log" ;;\n'
                         '    *) exit 1 ;;\n'
                         'esac')

    def run_defaults(self, shell):
        r = self.sh(shell, SCRIPT.read_text().replace('"/www$page"', f'"{self.www}$page"'))
        self.assertEqual(0, r.returncode, r.stderr)

    def error_page(self):
        lines = [l for l in self.conf.read_text().splitlines() if l.startswith('uhttpd.main.error_page=')]
        return lines[-1].split('=', 1)[1] if lines else None

    def test_points_uhttpd_at_the_console_once(self):
        for shell in self.each_shell():
            self.conf.write_text("uhttpd.main=uhttpd\n")
            (self.tmp / 'commit.log').unlink(missing_ok=True)
            self.run_defaults(shell)
            self.assertEqual('/f50/index.html', self.error_page())
            self.assertEqual('uhttpd\n', (self.tmp / 'commit.log').read_text())
            # the next image's first boot: already set, nothing committed
            self.run_defaults(shell)
            self.assertEqual('uhttpd\n', (self.tmp / 'commit.log').read_text())

    def test_an_error_page_of_the_users_stays(self):
        for shell in self.each_shell():
            self.conf.write_text("uhttpd.main=uhttpd\nuhttpd.main.error_page='/mine.html'\n")
            self.run_defaults(shell)
            self.assertEqual("'/mine.html'", self.error_page())
            self.assertFalse((self.tmp / 'commit.log').exists())

    def test_an_image_without_the_console_or_uhttpd(self):
        for shell in self.each_shell():
            (self.www / 'f50' / 'index.html').unlink()
            self.run_defaults(shell)
            self.assertIsNone(self.error_page())
            (self.www / 'f50' / 'index.html').write_text('<!doctype html>')
            self.conf.write_text('')
            self.run_defaults(shell)
            self.assertIsNone(self.error_page())
            self.assertFalse((self.tmp / 'commit.log').exists())

    def test_runs_once_per_image_and_removes_itself(self):
        self.assertTrue(os.access(SCRIPT, os.X_OK))
        body = SCRIPT.read_text()
        self.assertTrue(body.startswith('#!/bin/sh\n'))
        self.assertTrue(body.rstrip().endswith('exit 0'))


class Layout(unittest.TestCase):
    def test_front_page_is_a_link_to_the_console(self):
        install = (TOP / 'f50d' / 'openwrt' / 'install.sh').read_text()
        self.assertIn('ln -s f50/index.html "$R/www/index.html"', install)
        self.assertNotIn('http-equiv="refresh"', install)

    def test_files_under_f50_pages_at_the_root(self):
        web = TOP / 'f50-web'
        vite = (web / 'vite.config.ts').read_text()
        self.assertIn('base: command === "build" ? "/f50/" : "/"', vite)
        main = (web / 'src' / 'main.tsx').read_text()
        self.assertIn('<BrowserRouter>', main)
        self.assertNotIn('HashRouter', main)
        self.assertNotRegex((web / 'src' / 'lib' / 'ubus.ts').read_text(), r'location\.hash')
        # the same error_page on a device updated over SSH (make deploy)
        deploy = (web / 'make' / 'deploy.mk').read_text()
        self.assertIn('uhttpd.main.error_page=/f50/index.html', deploy)
        self.assertIn('ln -sf f50/index.html /www/index.html', deploy)
        # no route of the console may shadow what uhttpd serves itself
        routes = re.findall(r'<Route path="([^"]+)"', (web / 'src' / 'App.tsx').read_text())
        self.assertTrue(routes)
        for path in routes:
            self.assertNotRegex(path.lstrip('/'), r'^(f50|ubus|cgi-bin|luci-static)(/|$)', path)


if __name__ == '__main__':
    unittest.main()
