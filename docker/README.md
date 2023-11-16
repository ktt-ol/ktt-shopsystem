This Docker container can be used to build 32 bit
binaries for Debian stable.

# Prepare

Create the docker image with:
```bash
docker build \
        -t ktt-shop \
        --build-arg="DEBIAN_RELEASE=testing" \
        --build-arg="DEBIAN_ARCH=i386" \
        --build-arg="RUST_ARCH=i686-unknown-linux-gnu" \
        docker
```

# Build

Run the image with:
```bash
# change into the "ktt-shopsystem" directory
cd ..

docker run --rm -it -p 8080:8080 -v "$PWD":/mnt/ktt-shopsystem ktt-shop tmux
```

You have now a tmux terminal to run the program.
