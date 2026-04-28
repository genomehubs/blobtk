
FROM quay.io/pypa/manylinux2014_x86_64

# Install system build deps and tooling
RUN yum -y update && \
    yum -y install --setopt=tsflags=nodocs \
      openssl-devel \
      zlib-devel \
      perl \
      perl-ExtUtils-MakeMaker \
      make \
      gcc \
      gcc-c++ \
      which \
      pkgconfig \
      curl && \
    yum clean all && rm -rf /var/cache/yum

# Install rustup/cargo into a global location and add toolchain
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" && \
    curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --default-toolchain 1.91.0 && \
    rustup toolchain install 1.91.0 && \
    rustup component add rustfmt clippy --toolchain 1.91.0 && \
    rustup target add x86_64-unknown-linux-gnu --toolchain 1.91.0

# Optional: install sccache to speed repeated builds (best-effort)
RUN cargo install sccache --locked || true

ENV OPENSSL_NO_VENDOR=1 \
    OPENSSL_LIB_DIR=/usr/lib64 \
    OPENSSL_INCLUDE_DIR=/usr/include \
    LD_LIBRARY_PATH=/usr/lib64:$LD_LIBRARY_PATH
