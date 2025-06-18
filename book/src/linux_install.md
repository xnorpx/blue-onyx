# Linux Installation

This guide covers installing Blue Onyx on Linux using Docker, which is the recommended method for Linux deployment.

## Prerequisites

- Linux distribution with Docker support
- Docker Engine 20.10+ and Docker Compose (optional)
- Internet connection for downloading images and models

## Docker Installation (Recommended)

### Quick Start with Docker

```bash
# Pull the latest image
docker pull ghcr.io/xnorpx/blue_onyx:latest

# Run with default settings
docker run -p 32168:32168 ghcr.io/xnorpx/blue_onyx:latest
```

The service will be available at `http://localhost:32168`.

### Docker Run with Volume Mounts

For persistent model storage and configuration:

```bash
# Create directories for data persistence
mkdir -p ~/blue-onyx/{models,config,logs}

# Download models first (optional)
docker run --rm -v ~/blue-onyx/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --download-model-path /app/models --download-rt-detr2

# Run with persistent volumes
docker run -d \
  --name blue-onyx \
  -p 32168:32168 \
  -v ~/blue-onyx/models:/app/models \
  -v ~/blue-onyx/config:/app/config \
  -v ~/blue-onyx/logs:/app/logs \
  --restart unless-stopped \
  ghcr.io/xnorpx/blue_onyx:latest \
  --model /app/models/rt-detrv2-s.onnx \
  --log_path /app/logs/blue-onyx.log
```

### Docker Compose (Recommended for Production)

Create a `docker-compose.yml` file:

```yaml
version: '3.8'

services:
  blue-onyx:
    image: ghcr.io/xnorpx/blue_onyx:latest
    container_name: blue-onyx
    ports:
      - "32168:32168"
    volumes:
      - ./models:/app/models
      - ./config:/app/config
      - ./logs:/app/logs
      - ./processed_images:/app/processed_images
    environment:
      - RUST_LOG=info
    command: >
      --config /app/config/blue_onyx_config.json
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:32168/"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s

  # Optional: nginx reverse proxy
  nginx:
    image: nginx:alpine
    container_name: blue-onyx-proxy
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - ./ssl:/etc/nginx/ssl:ro
    depends_on:
      - blue-onyx
    restart: unless-stopped
```

**Configuration file (config/blue_onyx_config.json):**

```json
{
  "port": 32168,
  "model": "/app/models/rt-detrv2-s.onnx",
  "confidence_threshold": 0.5,
  "log_level": "Info",
  "log_path": "/app/logs/blue-onyx.log",
  "save_image_path": "/app/processed_images",
  "save_stats_path": "/app/logs/blue-onyx-stats.json",
  "force_cpu": false,
  "request_timeout": 30,
  "intra_threads": 2,
  "inter_threads": 2
}
```

**Start the services:**

```bash
# Start services
docker-compose up -d

# Download models
docker-compose exec blue-onyx blue_onyx --download-model-path /app/models --download-rt-detr2

# View logs
docker-compose logs -f blue-onyx
```

## Model Management

### Download Models with Docker

```bash
# Download all models
docker run --rm -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --download-model-path /app/models --download-all-models

# Download only RT-DETR models
docker run --rm -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --download-model-path /app/models --download-rt-detr2

# Download only YOLO5 specialized models
docker run --rm -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --download-model-path /app/models --download-yolo5

# List available models
docker run --rm ghcr.io/xnorpx/blue_onyx:latest --list-models
```

### Using Different Models

```bash
# Use a larger RT-DETR model
docker run -p 32168:32168 \
  -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --model /app/models/rt-detrv2-l.onnx

# Use a YOLO5 specialized model
docker run -p 32168:32168 \
  -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --model /app/models/delivery.onnx \
  --object-detection-model-type yolo5
```

## Advanced Configuration

### GPU Support (Experimental)

GPU support in Docker requires additional setup:

```bash
# Install nvidia-docker2 (NVIDIA GPUs only)
curl -s -L https://nvidia.github.io/nvidia-docker/gpgkey | sudo apt-key add -
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -s -L https://nvidia.github.io/nvidia-docker/$distribution/nvidia-docker.list | sudo tee /etc/apt/sources.list.d/nvidia-docker.list
sudo apt-get update && sudo apt-get install -y nvidia-docker2
sudo systemctl restart docker

# Run with GPU support
docker run --gpus all -p 32168:32168 \
  ghcr.io/xnorpx/blue_onyx:latest \
  --gpu_index 0
```

### Reverse Proxy with SSL

**nginx.conf example:**

```nginx
events {
    worker_connections 1024;
}

http {
    upstream blue-onyx {
        server blue-onyx:32168;
    }

    server {
        listen 80;
        server_name your-domain.com;
        return 301 https://$server_name$request_uri;
    }

    server {
        listen 443 ssl http2;
        server_name your-domain.com;

        ssl_certificate /etc/nginx/ssl/cert.pem;
        ssl_certificate_key /etc/nginx/ssl/key.pem;

        location / {
            proxy_pass http://blue-onyx;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            # Increase timeouts for large image uploads
            proxy_connect_timeout       60s;
            proxy_send_timeout          60s;
            proxy_read_timeout          60s;
            client_max_body_size        50M;
        }
    }
}
```

### Systemd Service (Alternative to Docker)

If you prefer not to use Docker, you can run Blue Onyx as a systemd service:

**blue-onyx.service:**

```ini
[Unit]
Description=Blue Onyx Object Detection Service
After=network.target

[Service]
Type=simple
User=blue-onyx
Group=blue-onyx
WorkingDirectory=/opt/blue-onyx
ExecStart=/opt/blue-onyx/blue_onyx --config /etc/blue-onyx/config.json
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

```bash
# Create user and directories
sudo useradd -r -s /bin/false blue-onyx
sudo mkdir -p /opt/blue-onyx /etc/blue-onyx /var/log/blue-onyx
sudo chown blue-onyx:blue-onyx /opt/blue-onyx /var/log/blue-onyx

# Download and install binary (you'll need to build from source)
# Copy binary to /opt/blue-onyx/blue_onyx

# Install and start service
sudo cp blue-onyx.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable blue-onyx
sudo systemctl start blue-onyx
```

## Docker Management

### Container Management

```bash
# View running containers
docker ps

# Stop Blue Onyx
docker stop blue-onyx

# Start Blue Onyx
docker start blue-onyx

# View logs
docker logs -f blue-onyx

# Execute commands in container
docker exec -it blue-onyx /bin/sh

# Remove container
docker rm blue-onyx

# Remove image
docker rmi ghcr.io/xnorpx/blue_onyx:latest
```

### Updates

```bash
# Pull latest image
docker pull ghcr.io/xnorpx/blue_onyx:latest

# Stop and remove old container
docker stop blue-onyx
docker rm blue-onyx

# Run new container
docker run -d --name blue-onyx -p 32168:32168 \
  -v ~/blue-onyx/models:/app/models \
  -v ~/blue-onyx/config:/app/config \
  --restart unless-stopped \
  ghcr.io/xnorpx/blue_onyx:latest
```

### With Docker Compose

```bash
# Update and restart
docker-compose pull
docker-compose up -d
```

## Monitoring and Logging

### View Logs

```bash
# Docker logs
docker logs -f blue-onyx

# Docker Compose logs
docker-compose logs -f blue-onyx

# Log files (if mounted)
tail -f logs/blue-onyx.log
```

### Health Checks

```bash
# Check if service is responding
curl -f http://localhost:32168/

# Check detailed stats
curl http://localhost:32168/stats

# Test detection
curl -X POST -F "image=@test.jpg" http://localhost:32168/detect
```

### Performance Monitoring

```bash
# Monitor container resources
docker stats blue-onyx

# Monitor with htop/top inside container
docker exec -it blue-onyx top
```

## Troubleshooting

### Common Issues

**Container Won't Start:**
```bash
# Check logs for errors
docker logs blue-onyx

# Check if port is in use
sudo netstat -tlpn | grep :32168

# Check file permissions
ls -la ~/blue-onyx/
```

**Model Download Failures:**
```bash
# Manual model download
docker run --rm -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --download-model-path /app/models --download-rt-detr2

# Check network connectivity
docker run --rm ghcr.io/xnorpx/blue_onyx:latest ping -c 3 huggingface.co
```

**Performance Issues:**
```bash
# Check CPU/memory usage
docker stats blue-onyx

# Use smaller model
docker run -p 32168:32168 \
  -v $(pwd)/models:/app/models \
  ghcr.io/xnorpx/blue_onyx:latest \
  --model /app/models/rt-detrv2-s.onnx \
  --force_cpu
```

### Debug Mode

```bash
# Run with debug logging
docker run -p 32168:32168 \
  -e RUST_LOG=debug \
  ghcr.io/xnorpx/blue_onyx:latest \
  --log_level Debug
```

## Security Considerations

### Network Security

- Run Blue Onyx behind a reverse proxy with SSL
- Use firewall rules to restrict access
- Consider VPN access for remote use

### Container Security

```bash
# Run with non-root user
docker run --user 1000:1000 \
  -p 32168:32168 \
  ghcr.io/xnorpx/blue_onyx:latest

# Use read-only filesystem
docker run --read-only \
  --tmpfs /tmp \
  -p 32168:32168 \
  ghcr.io/xnorpx/blue_onyx:latest
```

## Next Steps

- **[Getting Started](get_started.md)** - Basic usage and testing
- **[Configuration](configuration.md)** - Detailed configuration options
- **[Models](models.md)** - Choose the right model for your use case
- **[Blue Iris Integration](configure_blue_iris_5.md)** - Integrate with Blue Iris 5
