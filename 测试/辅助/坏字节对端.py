#!/usr/bin/env python3
"""回归用例「310接手回归_非法字节报错」的对端。

智语言的文本里造不出非法 UTF-8 字节，所以由这个脚本充当网络对端来发。
由 测试/双引擎回归.sh 在后台启动，整个回归期间常驻；用例连不上它时自行跳过。

协议（监听 127.0.0.1:18391，每条连接先读一行命令）：
  坏字节            → 回一段含非法 UTF-8 的字节后关闭连接
  坏请求 <端口>     → 向 127.0.0.1:<端口> 发一个请求体含非法 UTF-8 的 HTTP POST，
                      把对方响应的状态码（如 400）作为一行文本回给命令方
  好请求 <端口>     → 同上，但请求体是合法 UTF-8
"""
import socket
import socketserver
import sys

PORT = 18391
BAD = b"ab\xff\xfecd\n"


def post(port, body):
    with socket.create_connection(("127.0.0.1", port), timeout=5) as conn:
        head = (
            "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\n"
            "Content-Length: %d\r\nConnection: close\r\n\r\n" % len(body)
        )
        conn.sendall(head.encode("ascii") + body)
        data = b""
        while b"\r\n" not in data:
            chunk = conn.recv(4096)
            if not chunk:
                break
            data += chunk
    parts = data.split(b"\r\n", 1)[0].split()
    return parts[1].decode("ascii") if len(parts) >= 2 else "0"


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        self.connection.settimeout(10)
        try:
            words = self.rfile.readline().decode("utf-8", "replace").split()
            if not words:
                return
            if words[0] == "坏字节":
                self.wfile.write(BAD)
            elif words[0] in ("坏请求", "好请求") and len(words) == 2:
                body = b"\xff\xfe\x00" if words[0] == "坏请求" else "正常".encode("utf-8")
                self.wfile.write((post(int(words[1]), body) + "\n").encode("ascii"))
        except Exception as error:  # 对端异常不应让整个辅助进程退出
            print("坏字节对端：", error, file=sys.stderr)


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    with Server(("127.0.0.1", PORT), Handler) as server:
        server.serve_forever()
