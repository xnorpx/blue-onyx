## Architecture

The design of Blue Onyx is very simple. It implements the same HTTP API as other open-source object detection services for compatibility.

```http
/v1/vision/detection
```

The server is mainly implemented in [Rust](https://www.rust-lang.org/) but utilizes [ONNX](https://onnx.ai/) for inference which is written in C++. So all code is compiled and native.

The HTTP server is implemented in [axum](https://github.com/tokio-rs/axum) which utilizes [tokio](https://tokio.rs/) and runs async in one thread to handle requests. It can handle multiple requests at the same time. Each request is then put on a channel/queue to the worker thread. The worker thread handles the decoding of the image, resizing, and finally running the inference. Once this is done, the results are gathered, and a response is sent back to the task in the main thread that was handling the request.

<div style="display: flex; align-items: center;">
    <div style="flex: 1;">
        Most clients have a timeout limit, which means that if a client sends a request to Blue Onyx, they expect a result within a certain time frame. If this time expires, it indicates that we are either processing too slowly or overwhelming the server with requests.
        This can be visualized as a glass of water: the incoming water represents the requests, the size of the glass represents the timeout, and the straw represents the rate at which we process the requests.
    </div>
    <div style="flex: 1; text-align: center;">
        <img src="images/flow.jpg" alt="Flow" width="150"/>
    </div>
</div>

To ensure optimal performance, it's crucial to use a model that can handle the system's load efficiently. For instance, processing an image every 1-2 seconds might suffice for a single camera. However, with 20 cameras generating high traffic, the processing speed may need to be as fast as 50 milliseconds per image.

When setting up Blue Onyx, the queue size is adjusted based on your timeout (the size of the glass) and the processing speed (how fast we can suck out the water). If the system reaches its capacity, Blue Onyx will return errors and log warnings indicating it is over capacity. While the system will recover, it's essential to ensure sufficient resources and fast hardware to manage the system's load effectively.

Each Blue Onyx instance runs one model. If a user wants to run multiple models on one machine, one can launch multiple Blue Onyx instances running on different ports. The only consideration would be if one run on CPU to assign a subset of cores to each server. For GPU the scheduling is handled by the GPU and multiple processes and threads can share GPU if needed.

- Blue Onyx Server 1 with model 1 on port 32168
- Blue Onyx Server 2 with model 2 on port 32167

This design allows users to host multiple models and lets the system handle scheduling and resources.