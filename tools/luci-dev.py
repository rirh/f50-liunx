#!/usr/bin/env python3
"""Serve local LuCI assets and proxy the rest to an OpenWrt device (Python 3.8+).

    make dev [DEVICE=http://192.168.77.1] [PORT=27547]

Only browser assets are local. Login and RPC calls still reach the real device.
"""
import argparse
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import mimetypes
from pathlib import Path
import shutil
from urllib.parse import unquote, urlsplit


TOP = Path(__file__).resolve().parent.parent
WEB_ROOTS = (
    TOP / 'openwrt/luci-app-mu300/htdocs',
    TOP / 'openwrt/overlay/www',
)
HOP_HEADERS = {
    'connection', 'keep-alive', 'proxy-authenticate', 'proxy-authorization',
    'te', 'trailer', 'transfer-encoding', 'upgrade',
}
CACHE_HEADERS = {'cache-control', 'expires', 'etag', 'last-modified'}


def hop_headers(headers):
    return HOP_HEADERS | {
        token.strip().lower()
        for value in headers.get_all('Connection', [])
        for token in value.split(',')
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.dispatch()

    do_HEAD = do_POST = do_PUT = do_PATCH = do_DELETE = do_OPTIONS = do_GET

    def log_request(self, code='-', size='-'):
        # Query strings can contain session or download tokens.
        self.log_message('%s %s %s', self.command, urlsplit(self.path).path, code)

    def dispatch(self):
        request = urlsplit(self.path)
        if request.scheme or request.netloc:
            self.send_error(400, 'Use a local path, not a proxy URL')
            return
        if self.headers.get('Upgrade'):
            self.send_error(501, 'Use the device directly for WebSocket services')
            return
        if self.headers.get('Transfer-Encoding'):
            self.send_error(501, 'A Content-Length is required for uploads')
            return
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if length < 0:
                raise ValueError
        except ValueError:
            self.send_error(400, 'Invalid Content-Length')
            return

        path = unquote(request.path)
        if self.command in ('GET', 'HEAD') and path == '/' and not length:
            self.send_response(302)
            self.send_header('Location', '/cgi-bin/luci/admin/home')
            self.send_header('Content-Length', '0')
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            return
        if self.command in ('GET', 'HEAD') and path.startswith('/luci-static/') and not length:
            for root in WEB_ROOTS:
                try:
                    local = (root / path.lstrip('/')).resolve()
                    local.relative_to(root.resolve())
                except (ValueError, OSError, RuntimeError):
                    self.send_error(403, 'Invalid asset path')
                    return
                if local.is_file():
                    self.serve_asset(local)
                    return
        self.proxy(length)

    def serve_asset(self, path):
        try:
            source = path.open('rb')
        except OSError:
            self.send_error(404, 'Local asset is not readable')
            return
        with source:
            self.send_response(200)
            kind = mimetypes.guess_type(str(path))[0] or 'application/octet-stream'
            self.send_header('Content-Type', kind)
            self.send_header('Content-Length', str(path.stat().st_size))
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            if self.command != 'HEAD':
                try:
                    shutil.copyfileobj(source, self.wfile)
                except (BrokenPipeError, ConnectionResetError):
                    pass  # A refresh can cancel the previous response.

    def proxy(self, length):
        device = self.server.device
        connection = http.client.HTTPConnection(device.hostname, device.port, timeout=60)
        sent = False
        try:
            # Keep the browser's Host/Origin together for LuCI login and CSRF checks.
            connection.putrequest(self.command, self.path, skip_host=True, skip_accept_encoding=True)
            excluded = hop_headers(self.headers) | {'if-none-match', 'if-modified-since'}
            for key, value in self.headers.items():
                if key.lower() not in excluded:
                    connection.putheader(key, value)
            connection.putheader('Connection', 'close')
            connection.endheaders()
            while length:
                block = self.rfile.read(min(length, 65536))
                if not block:
                    raise ConnectionError('Upload ended before Content-Length')
                connection.send(block)
                length -= len(block)

            response = connection.getresponse()
            self.send_response(response.status, response.reason)
            excluded = hop_headers(response.headers) | CACHE_HEADERS
            for key, value in response.getheaders():
                if key.lower() in excluded:
                    continue
                if key.lower() == 'location':
                    location = urlsplit(value)
                    if location.netloc == device.netloc and location.scheme in ('', 'http'):
                        value = location._replace(scheme='', netloc='').geturl() or '/'
                self.send_header(key, value)
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            sent = True
            if self.command != 'HEAD':
                while True:
                    block = response.read1(65536)
                    if not block:
                        break
                    self.wfile.write(block)
        except (BrokenPipeError, ConnectionResetError):
            self.close_connection = True
        except (OSError, http.client.HTTPException) as error:
            if not sent:
                self.send_error(502, 'Cannot reach OpenWrt: %s' % error)
            else:
                self.close_connection = True
        finally:
            connection.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--device', default='http://192.168.77.1', help='OpenWrt HTTP origin')
    parser.add_argument('--port', type=int, default=27547, help='Local port (default: 27547)')
    args = parser.parse_args()
    try:
        device = urlsplit(args.device)
        port = device.port
        if (device.scheme != 'http' or not device.hostname or device.username is not None
                or device.password is not None or device.path not in ('', '/')
                or device.query or device.fragment or port == 0):
            raise ValueError
    except ValueError:
        parser.error('--device must be an HTTP origin, for example http://192.168.77.1')
    if not 1 <= args.port <= 65535:
        parser.error('--port must be between 1 and 65535')
    try:
        server = ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
    except OSError as error:
        parser.exit(1, 'Cannot start local proxy: %s\n' % error)
    server.device = device
    print('LuCI development: http://127.0.0.1:%d/' % args.port, flush=True)
    print('Backend: %s (login and actions use the real device)' % args.device, flush=True)
    print('Local JS/CSS: edit the repository files, save and refresh. Ctrl+C to stop.', flush=True)
    print('Use the device URL for the web terminal on its separate port.', flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print('\nStopped.')
    finally:
        server.server_close()


if __name__ == '__main__':
    main()
