#!/usr/bin/python3

import argparse
import docker
from docker.client import DockerClient
from docker.models.containers import Container
from docker.models.images import Image
from docker.types import Mount
import os
import os.path
from typing import Optional


CONTAINER = 'udrop-server'
IMAGE = 'debian:bookworm-slim'


def server_kill(client: DockerClient, args) -> None:
    try:
        container = client.containers.get(CONTAINER)

        print('Killing container…')
        container.kill()
    except docker.errors.NotFound:
        print('Container not running!')


def server_create(client: DockerClient) -> None:
    try:
        image = client.images.get(IMAGE)
    except docker.errors.APIError:
        image = client.images.pull(IMAGE)

    print('Creating container…')

    return client.containers.create(
        image.id,
        command='/bin/sh',
        auto_remove=True,
        environment=[
            "RUST_BACKTRACE=full",
        ],
        name=CONTAINER,
        mounts=[
            Mount(
                '/src',
                type='bind',
                source=os.path.abspath(os.path.dirname(__file__)),
                propagation='rslave',
            ),
        ],
        tty=True,
        working_dir='/src',
    )


def server_run(client: DockerClient, args) -> None:
    try:
        container = client.containers.get(CONTAINER)
        if container.status == 'running':
            print('Container already running!')

            return
    except docker.errors.NotFound:
        container = server_create(client)

    print('Starting container…')

    container.start()
    cmd = ['target/debug/examples/udrop', '-l', '0.0.0.0']
    res = container.exec_run(cmd, tty=True, stream=True)

    try:
        for message in res.output:
            print(message.decode('utf-8'), end='')
    except KeyboardInterrupt:
        server_kill(client, args)


def server_get_ip(client: DockerClient) -> str:
    container = client.containers.get(CONTAINER)

    return container.attrs['NetworkSettings']['Networks']['bridge']['IPAddress']


def server_restart(client: DockerClient, args) -> None:
    try:
        container = client.containers.get(CONTAINER)
        print('Restarting container…')
        container.restart(timeout=3)
    except docker.errors.NotFound:
        print('Container not running!')


def server_print_ip(client: DockerClient, args) -> None:
    try:
        print(server_get_ip(client))
    except docker.errors.NotFound:
        print('Container not running!')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(title='subcommands')

    server_parser = subparsers.add_parser('server')
    server_subparsers = server_parser.add_subparsers(title='subcommands',
                                                     required=True)

    server_subparsers.add_parser('run').set_defaults(func=server_run)
    server_subparsers.add_parser('kill').set_defaults(func=server_kill)
    server_subparsers.add_parser('ip').set_defaults(func=server_print_ip)
    server_subparsers.add_parser('restart').set_defaults(func=server_restart)

    args = parser.parse_args()

    args.func(docker.from_env(), args)
