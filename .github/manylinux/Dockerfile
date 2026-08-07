
FROM quay.io/pypa/manylinux2014_x86_64

# 1. FIX THE CENTOS 7 EOL REPOS (Point everything to the archived Vault)
RUN sed -i 's/mirror.centos.org/vault.centos.org/g' /etc/yum.repos.d/*.repo && \
    sed -i 's/^#.*baseurl=http/baseurl=http/g' /etc/yum.repos.d/*.repo && \
    sed -i 's/^mirrorlist=http/#mirrorlist=http/g' /etc/yum.repos.d/*.repo

# 2. Install system build deps and tooling
RUN yum -y update && \
    yum -y install --setopt=tsflags=nodocs \
    openssl-devel \
    zlib-devel \
    perl \
    perl-IPC-Cmd \
    perl-ExtUtils-MakeMaker \
    make \
    gcc \
    gcc-c++ \
    bzip2-devel \
    xz-devel \
    which \
    pkgconfig \
    curl

# 3. Enable CentOS Software Collections (SCL) 
RUN yum install -y centos-release-scl

# 4. FIX THE SCL REPOS TOO (The previous step creates new .repo files that also point to dead links)
RUN sed -i 's/mirror.centos.org/vault.centos.org/g' /etc/yum.repos.d/*.repo && \
    sed -i 's/^#.*baseurl=http/baseurl=http/g' /etc/yum.repos.d/*.repo && \
    sed -i 's/^mirrorlist=http/#mirrorlist=http/g' /etc/yum.repos.d/*.repo

# 5. Clear package manager cache and build fresh index maps
RUN yum clean all && yum makecache

# 6. Install the LLVM / Clang developer kit toolset safely
RUN yum install -y llvm-toolset-7-clang-devel llvm-toolset-7-libclang-devel && \
    yum clean all && rm -rf /var/cache/yum

# 7. Map out the environment bindings for your Rust build stage
ENV LIBCLANG_PATH=/opt/rh/llvm-toolset-7/root/usr/lib64 \
    CLANG_PATH=/opt/rh/llvm-toolset-7/root/usr/bin/clang \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# 8. Install rustup/cargo into a global location and add toolchain
RUN mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" && \
    curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --default-toolchain 1.91.0 && \
    rustup toolchain install 1.91.0 && \
    rustup component add rustfmt clippy --toolchain 1.91.0 && \
    rustup target add x86_64-unknown-linux-gnu --toolchain 1.91.0

# 9. Install sccache to speed repeated builds (best-effort)
RUN cargo install sccache --locked || true

# 10. Set OpenSSL environment variables
ENV OPENSSL_NO_VENDOR=1 \
    OPENSSL_LIB_DIR=/usr/lib64 \
    OPENSSL_INCLUDE_DIR=/usr/include \
    LD_LIBRARY_PATH=/usr/lib64:$LD_LIBRARY_PATH
