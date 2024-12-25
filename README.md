
<div align="center">
<img src="assets/blue_onyx.gif" alt="blue_onyx"/>
</div>

# Object Detection Service

## TL;DR

- Windows only (for now)
- CPU and GPU inference (NVIDIA, AMD and Intel)
- [ONNX Inference](https://github.com/onnx/onnx)
- [Direct ML Endpoint](https://github.com/microsoft/DirectML)
- [RT-DETR-V2 Model](https://github.com/lyuwenyu/RT-DETR/tree/main/rtdetrv2_pytorch)
- [ONNX RT-DETR-V2 Models](https://huggingface.co/xnorpx/rt-detr2-onnx)

## Install with THE one mighty oneliner

```powershell
 powershell -NoProfile -Command "curl 'https://github.com/xnorpx/blue-onyx/releases/latest/download/install_latest_blue_onyx.ps1' -o 'install_latest_blue_onyx.ps1'; Unblock-File '.\install_latest_blue_onyx.ps1'; powershell.exe -ExecutionPolicy Bypass -File '.\install_latest_blue_onyx.ps1'"
 ```

## I don't trust scripts I want to install myself

- Download latest release
- Unzip
- Download models (blue_onyx.exe --download-model-path .)
- blue_onyx.exe to run service
- test_blue_onyx.exe to test service
- blue_onyx_benchmark.exe for benchmark and model testing

## Tips

Help:
```bash
blue_onyx.exe --help
```

Download models:
```bash 
blue_onyx.exe --download-model-path .
```

Run service with larger model:
```bash 
blue_onyx.exe --model rt-detrv2-x.onnx
Initializing detector with model: "rt-detrv2-x.onnx"
```

Benchmark GPU
```bash
blue_onyx_benchmark.exe --repeat 100 --save-stats-path .
Device Name,Version,Type,Platform,EndpointProvider,Images,Total [s],Min [ms],Max [ms],Average [ms],FPS
Intel(R) Iris(R) Xe Graphics,0.1.0,GPU,Windows,DML,100,14.3,116.8,168.3,143.2,7.0
```

Benchmark CPU
```bash
blue_onyx_benchmark.exe --repeat 100 --save-stats-path . --force-cpu
Device Name,Version,Type,Platform,EndpointProvider,Images,Total [s],Min [ms],Max [ms],Average [ms],FPS
12th Gen Intel(R) Core(TM) i7-1265U,0.1.0,CPU,Windows,CPU,100,28.2,239.6,398.2,281.5,3.6
```

Test Service
```bash
blue_onyx.exe
```

Then run in another terminal do 100 requests with 100 ms interval
```bash
test_blue_onyx.exe --number-of-requests 100 --interval 100
```
    
Test image and save image with boundary box use --image to specify your own image.
```bash
blue_onyx_benchmark.exe --save-image-path .
```

<div align="center">
<img src="assets/dog_bike_car_od.jpg" alt="dog_bike_car_od"/>
</div>
