This Docker container can be used to build 32 bit
binaries for Debian stable.

# Prepare

Create the docker image with:
```bash
docker build -t ktt-shop .
```

# Build

Run the image with:
```bash
# change into the "ktt-shopsystem" directory
cd ..

docker run --rm -it -p 8080:8080 -v "$PWD":/mnt/ktt-shopsystem ktt-shop tmux
```

You have now a tmux terminal to run the program.
