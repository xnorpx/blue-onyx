FROM debian:trixie-slim

LABEL maintainer="xnorpx@outlook.com"
LABEL description="Blue Onyx docker container"

ENV TARGET_FOLDER=/models

# Install dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends openssl && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --no-log-init blueonyx

# Set working directory
WORKDIR /app

# Copy application files and set ownership
COPY --chown=blueonyx:blueonyx blue_onyx libonnxruntime.so ./

# Copy model files and set ownership
COPY --chown=blueonyx:blueonyx models/* ./

# Switch to non-root user
USER blueonyx

# Expose port
EXPOSE 32168

# Define entrypoint
ENTRYPOINT ["./blue_onyx"]
