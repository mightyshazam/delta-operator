target = local(
    """
    platform=`uname -p`
    if [[ $platform == 'arm' ]]; then
        echo "aarch64-unknown-linux-gnu"
    else
        echo "x86_64-unknown-linux-gnu"
    fi
    """)
cargo_config = local(
    """
    os=`uname`
    target=`uname -p`
    if [[ $os == 'Linux' ]]; then
        echo ""
    else
        if [[ $target == 'arm' ]]; then
            echo "--config target.aarch64-unknown-linux-gnu.linker=\\\"aarch64-linux-gnu-gcc\\\""
        else
            echo "--config target.x86_64-unknown-linux-gnu.linker=\\\"x86_64-unknown-linux-gnu-gcc\\\""
        fi
    fi
    """,
    env={
        'CARGO_TARGET': target
    }
)
local_resource(
    'build-controller',
    'echo $CARGO_CONFIG $CARGO_TARGET && cargo build --target $CARGO_TARGET $CARGO_CONFIG',
    env={
        'CARGO_TARGET': target,
        'CARGO_CONFIG': cargo_config
    },
    deps=['controller', 'crd', 'worker', 'Cargo.toml', 'Cargo.lock'],
    ignore=['.github', 'README.md', 'target', 'manifests', '.gitignore'],
)
local_resource('storage-init',
'bash setup.sh',
deps=['manifests/kubernetes/development/infrastructure/storage-init'],
dir='manifests/kubernetes/development/infrastructure/storage-init',
resource_deps=['azurite', 'localstack'])
docker_context = ("target/%s/debug" % target).replace('\n', '')
docker_build(
    'docker-delta-operator',
    context=docker_context,
    dockerfile_contents="""
FROM debian:bookworm-slim

WORKDIR /build
RUN apt update \
    && apt install -y openssl ca-certificates \
    && apt clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

COPY delta-operator .
ENTRYPOINT ["/build/delta-operator"]
""",
    ignore=['.fingerprint', 'build', 'deps', 'examples', 'incremental'],
    only=['delta-operator'],
    match_in_env_vars=True)
docker_build(
    'docker-delta-operator-worker',
    context=docker_context,
    dockerfile_contents="""
FROM ubuntu

WORKDIR /build
RUN apt update \
    && apt install -y openssl ca-certificates \
    && apt clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

COPY delta-operator-worker .
ENTRYPOINT ["/build/delta-operator-worker"]
""",
    ignore=['.fingerprint', 'build', 'deps', 'examples', 'incremental'],
    only=['delta-operator-worker'],
    match_in_env_vars=True)

k8s_yaml(kustomize('manifests/kubernetes/development'))
k8s_resource('azurite', port_forwards=['10000:10000'])
k8s_resource('localstack', port_forwards=['4566:4566'])
