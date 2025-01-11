# Blue Onyx Object Detection Service

Blue Onyx is a simple object detection service built on top of the ONNX runtime, which provides the inference engine.

Blue Onyx was created out of frustration with other open-source object detection services, which were often a mix of hastily assembled Python code under HTTPS endpoints.

Hence, the idea arose: can this be done in a simpler, more robust way than other solutions?

To avoid falling into the same feature creep traps as other solutions, Blue Onyx is designed to solve limited problems. Its main goals are to be stable, easy to upgrade, and to have decent performance over a wide range of normal consumer hardware.

With this philosophy in mind, Blue Onyx is designed with certain limitations. It is unlikely to support:

- Specialized NPU/TPU hardware
- Dynamic switching of multiple models at runtime
- Deployment and installation scripts for all different deployment systems

These constraints help maintain the simplicity and robustness of the service.

For example, if you are running an x86 Windows or standard Linux distribution with a consumer CPU/GPU combo and want a stable object detection service that just works with new state-of-the-art object detection models, then Blue Onyx might be right for you.

However, if you are running an ARM-based unRAID home server with Hailo and Coral TPUs, with Proxmox in Docker containers with an NVIDIA datacenter Tesla GPU, and you are trying to optimize for power consumption and inference engine performance measured in nanoseconds, then you are most likely looking for something custom for your setup, and Blue Onyx is most likely not the correct choice for you.

The design of Blue Onyx is very simple. It implements the same HTTP API as other open-source object detection services for compatibility. It is mainly implemented in Rust, which makes it very robust. The HTTP server runs async in one thread to receive requests. Each request is then put on a channel/queue to the worker thread. The worker thread handles the decoding of the image, resizing, and finally running the inference. Once this is done, the results are gathered, and a response is sent back to the task in the main thread that was handling the request.

Each Blue Onyx instance runs one model. If a user wants to run multiple models on one machine, they can launch multiple Blue Onyx instances running on different ports.

- Blue Onyx Server 1 with model 1 on port 32168
- Blue Onyx Server 2 with model 2 on port 32167

This design allows users to host multiple models and lets the system handle scheduling and resources.