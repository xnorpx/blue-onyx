# Linux OpenVINO build

This build enables Blue Onyx GPU inference through the ONNX Runtime OpenVINO
execution provider. It targets Intel GPU device `GPU.<gpu-index>`, uses FP16,
and uses one latency-focused stream. OpenVINO support is opt-in through the
`openvino` Cargo feature, so standard Linux builds retain CPU inference.

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

Set `BLUE_ONYX_OPENVINO_CACHE_DIR` to enable the compiled-model cache. The
OpenVINO Docker image sets it to `/app/config/openvino-cache`.

The example image is tagged `blue-onyx-openvino:0.8.0-openvino2`. Its Compose
definition is stored in `deployment/docker-compose.yml`. The container runs as
UID/GID 1000 for bind-mounted configuration and log directories.
