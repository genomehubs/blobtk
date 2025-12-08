FROM rust:1.82 as builder
WORKDIR /usr/src/blobtk
COPY . .
RUN apt-get update && apt-get install -y cmake python3 && rm -rf /var/lib/apt/lists/*
# RUN cargo install --path .
RUN RUSTFLAGS="-C target-feature=+crt-static" cargo install --path . --target x86_64-unknown-linux-gnu

FROM debian:bullseye-slim
# RUN sed -r -i 's/^deb(.*)$/deb\1 contrib/g' /etc/apt/sources.list \
#     && apt-get update \
#     && apt-get install -y ttf-mscorefonts-installer \
#     && rm -rf /var/lib/apt/lists/*
RUN apt-get update \
    && apt-get install -y curl fontconfig procps unzip \
    && rm -rf /var/lib/apt/lists/*
RUN curl -Ls "https://gwfh.mranftl.com/api/fonts/roboto?download=zip&subsets=latin&variants=700,regular,italic,700italic" > roboto.zip \
    && unzip roboto.zip \
    && cp roboto*.ttf  /usr/share/fonts/ \
    && rm -r roboto* \
    && dpkg-reconfigure fontconfig-config
COPY --from=builder /usr/local/cargo/bin/blobtk /usr/local/bin/blobtk
# RUN RUN fc-cache -f -v
CMD ["blobtk"]