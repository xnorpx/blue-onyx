# Linux OpenVINO build

This branch enables Blue Onyx GPU inference through the ONNX Runtime OpenVINO
execution provider. It targets Intel GPU device `GPU.<gpu-index>`, uses FP16,
one latency-focused stream, and stores compiled-model data under
`/app/config/openvino-cache`.

Build the image with:

```sh
docker build -f Dockerfile.openvino -t blue-onyx-openvino:0.8.0-openvino2 .
```

The container requires the Intel render device:

```sh
docker run --device /dev/dri/renderD128:/dev/dri/renderD128 \
  blue-onyx-openvino:0.8.0-openvino2
```

Pass `--force-cpu` to retain the upstream CPU execution path.

The deployed image is tagged `blue-onyx-openvino:0.8.0-openvino2`. Its
production Compose definition is stored in `deployment/docker-compose.yml`.
The container runs as UID/GID 1000 to remain compatible with the existing
Blue Onyx bind-mounted configuration and log directories.
