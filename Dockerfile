FROM debian:trixie-slim

LABEL maintainer="xnorpx@outlook.com"
LABEL description="Blue Onyx docker container"

# Install dependencies and CUDA/cuDNN
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        openssl \
        wget \
        ca-certificates \
        gnupg && \
    # Add NVIDIA package repository using modern method
    wget -qO - https://developer.download.nvidia.com/compute/cuda/repos/debian12/x86_64/3bf863cc.pub | gpg --dearmor -o /usr/share/keyrings/cuda-archive-keyring.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/cuda-archive-keyring.gpg] https://developer.download.nvidia.com/compute/cuda/repos/debian12/x86_64/ /" > /etc/apt/sources.list.d/cuda.list && \
    apt-get update && \
    # Install CUDA 12.8 runtime libraries
    apt-get install -y --no-install-recommends \
        cuda-cudart-12-8 \
        cuda-compat-12-8 \
        libcublas-12-8 \
        libcufft-12-8 \
        libcurand-12-8 \
        libcusolver-12-8 \
        libcusparse-12-8 \
        libnpp-12-8 \
        libnvjpeg-12-8 \
        # Install cuDNN 9 from CUDA repository
        libcudnn9-cuda-12 && \
    # Clean up
    apt-get remove -y wget gnupg && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --no-log-init blueonyx

# Set working directory
WORKDIR /app

# Copy application files and set ownership
COPY --chown=blueonyx:blueonyx blue_onyx libonnxruntime.so libonnxruntime_providers_cuda.so* libonnxruntime_providers_shared.so* ./

# Switch to non-root user
USER blueonyx

# Expose port
EXPOSE 32168

# Define entrypoint
ENTRYPOINT ["./blue_onyx"]
