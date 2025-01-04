FROM debian:trixie 

ENV TARGET_FOLDER=/models

RUN apt-get update && apt-get install -y openssl && rm -rf /var/lib/apt/lists/*

RUN useradd -m blueonyx

WORKDIR /app
COPY blue_onyx /app/
COPY libonnxruntime.so /app/
COPY models/* /app/

RUN chown -R blueonyx:blueonyx /app

USER blueonyx

EXPOSE 32168

ENTRYPOINT ["./blue_onyx"]
