import docker

from scenarios import scenarios

# Initialize docker client
client = docker.DockerClient(base_url="unix://var/run/docker.sock")
network = client.networks.create(
    "libdrop_test_network", driver="bridge", attachable=True
)

# Common parameters extracted from x-peer
COMMON_VOLUMES = {
    "/home/lukaspukenis/Development/libdrop/": {  # TODO
        "bind": "/libdrop",
        "mode": "rw",
    }
}
COMMON_WORKING_DIR = "/libdrop/test"
COMMON_CAP_ADD = ["NET_ADMIN"]

# Network creation
ipv4_net = client.networks.create(
    name="interneciux",
    driver="bridge",
    ipam=docker.types.IPAMConfig(
        pool_configs=[docker.types.IPAMPool(subnet="172.30.0.0/16")]
    ),
)

ipv6_net = client.networks.create(
    name="interneciux-v6",
    driver="bridge",
    options={"com.docker.network.enable_ipv6": "true"},
    ipam=docker.types.IPAMConfig(
        pool_configs=[
            docker.types.IPAMPool(
                subnet="fd3e:0e6d:45fe:b0c2::/64", gateway="fd3e:0e6d:45fe:b0c2::1"
            )
        ]
    ),
)

i = 0
for scenario in scenarios:
    print(f"Starting scenario {scenario.id()}")
    i += 1

    for runner in scenario.runners():
        hostname = f"{runner}-{i}"
        print(f"Starting {hostname}...")
        LIB_PATH = "../target/debug/libnorddrop.so"  # TODO makefie
        cmd = f"sh -c './run.py --runner={hostname} --scenario={scenario.id()} --lib={LIB_PATH}'"

        env = [
            "RUST_BACKTRACE=1",
            f"DROP_PEER_REN=DROP_PEER_REN-{i}",  # TODO
            f"DROP_PEER_STIMPY=DROP_PEER_STIMPY-{i}",  # TODO
            f"DROP_PEER_GEORGE=DROP_PEER_GEORGE-{i}",  # TODO
        ]

        print(f"Running {cmd}", flush=True)
        container = client.containers.run(
            image="libdrop-test-image",  # TODO: push the image
            name=f"{hostname}",
            command=cmd,
            volumes=COMMON_VOLUMES,
            working_dir=COMMON_WORKING_DIR,
            cap_add=COMMON_CAP_ADD,
            environment=env,
            hostname=f"{hostname}",
            detach=True,
            network="libdrop_test_network",
        )
