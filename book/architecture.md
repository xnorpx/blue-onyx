## Architecture

The design of Blue Onyx is very simple. It implements the same HTTP API as other open-source object detection services for compatibility. It is mainly implemented in Rust, which makes it very robust. The HTTP server runs async in one thread to receive requests. Each request is then put on a channel/queue to the worker thread. The worker thread handles the decoding of the image, resizing, and finally running the inference. Once this is done, the results are gathered, and a response is sent back to the task in the main thread that was handling the request.

Each Blue Onyx instance runs one model. If a user wants to run multiple models on one machine, they can launch multiple Blue Onyx instances running on different ports.

- Blue Onyx Server 1 with model 1 on port 32168
- Blue Onyx Server 2 with model 2 on port 32167

This design allows users to host multiple models and lets the system handle scheduling and resources.