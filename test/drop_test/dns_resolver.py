from typing import Callable
import os
import socket


class DNSResolver:  # TODO: the name is a lie, it's more of a PeerResolver
    def __init__(self):
        self._peers = {}
        self._cache = {}

        # peers do not know initially how their peers are named, and because
        # everyone lives in the same network we need to dynamically allocate
        # hostnames. This mapping maps those peers to their hostnames based on
        # environment variables.
        peer_env_vars = [
            "DROP_PEER_REN",
            "DROP_PEER_STIMPY",
            "DROP_PEER_GEORGE",
            "DROP_PEER_REN6",
            "DROP_PEER_STIMPY6",
            "DROP_PEER_GEORGE6",
        ]

        print(
            f"Initializing DNS resolver. Looking for peers in {peer_env_vars}",
            flush=True,
        )
        for peer_env_var in peer_env_vars:
            if peer_env_var in os.environ:
                peer = os.environ[peer_env_var]
                self._peers[peer_env_var] = peer
                print(f"Found peer {peer_env_var}={peer}", flush=True)

        if len(self._peers) == 0:
            print("No peers found in DNSResolver", flush=True)

    def resolve(self, peer: str) -> str:
        hostname = self._peers[peer]

        if hostname in self._cache:
            return self._cache[hostname]

        import socket

        ip = socket.gethostbyname(hostname)
        self._cache[hostname] = ip
        return ip

    def reverse_lookup(self, ip: str) -> str:
        for hostname, cached_ip in self._cache.items():
            if cached_ip == ip:
                return hostname.split("-")[0]  # TODO


dns_resolver = DNSResolver()
