# NVIDIA CUDA support on Linux

Blue Onyx can use the ONNX Runtime CUDA execution provider on Linux. CUDA is a
separate build feature so the regular CPU image does not require NVIDIA
libraries.

## Container build (recommended)

Requirements:

- Linux x86_64 with a supported NVIDIA GPU and driver
- Docker Engine with the NVIDIA Container Toolkit configured

The supplied CUDA 12.8/cuDNN 9.8 image requires an NVIDIA R570-series driver
or newer. Its release build includes compute capabilities 5.2, 6.0, 6.1, 7.0,
7.2, 7.5, 8.0, 8.6, 8.7, 8.9, 9.0, 10.0, 10.1, and 12.0, covering discrete
Maxwell through Blackwell GPUs. The published image targets x86_64; Jetson
devices require a separate aarch64/JetPack build.

Verify the container runtime first:

```bash
docker run --rm --gpus all nvidia/cuda:12.8.1-base-ubuntu24.04 nvidia-smi
```

Build and run Blue Onyx:

```bash
docker build -f Dockerfile.cuda -t blue-onyx-cuda:0.8.0 .
docker run --rm --gpus all -p 32168:32168 blue-onyx-cuda:0.8.0
```

Published releases are tagged as
`ghcr.io/xnorpx/blue_onyx:<version>-cuda` and
`ghcr.io/xnorpx/blue_onyx:latest-cuda`.

Or use the included Compose file:

```bash
mkdir -p config logs
sudo chown -R 10001:10001 config logs
docker compose -f deployment/docker-compose.cuda.yml up -d
```

Open `http://localhost:32168/stats` and confirm that **Execution Provider** is
`CUDA(GPU 0)`. Use `--gpu-index N` to select another GPU. Pass `--force-cpu` to
run the CUDA image on CPU.

CUDA provider registration is strict. When CUDA, cuDNN, the selected GPU, or an
ONNX Runtime provider library cannot be loaded, detector initialization fails
with the provider error instead of reporting GPU inference while silently using
the CPU.

The default image builds ONNX Runtime 1.29 with CUDA 12.8 and cuDNN 9. Override
`CUDA_VERSION`, `ONNXRUNTIME_VERSION`, or `CUDA_ARCHITECTURES` with Docker build
arguments when a different supported combination is required.
Custom architecture lists must contain at least one architecture at or above
7.5 because ONNX Runtime builds its CUDA LLM support as a separate object
library with that minimum target.

## Native build

Install a CUDA toolkit and cuDNN version supported by ONNX Runtime 1.29, then
set their roots if they are not in the default locations:

```bash
export CUDA_HOME=/usr/local/cuda
export CUDNN_HOME=/usr
# Include at least one architecture >= 75. ONNX Runtime 1.29 links its
# CUDA LLM support only when such an architecture is present.
export BLUE_ONYX_CUDA_ARCHITECTURES="61;75"
cargo build --release --features cuda
```

`BLUE_ONYX_CUDA_ARCHITECTURES` is optional. It accepts CMake's semicolon-separated
architecture list. A native CUDA build emits `libonnxruntime.so`,
`libonnxruntime_providers_shared.so`, and `libonnxruntime_providers_cuda.so`
beside the Blue Onyx binaries; keep these files together when deploying.

The `cuda` and `openvino` Cargo features represent alternative Linux GPU
images. If both are selected, CUDA takes precedence.
