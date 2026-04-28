FROM quay.io/pypa/manylinux2014_x86_64

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
    pkgconfig && \
    yum clean all

ENV OPENSSL_NO_VENDOR=1 \
    OPENSSL_LIB_DIR=/usr/lib64 \
    OPENSSL_INCLUDE_DIR=/usr/include \
    LD_LIBRARY_PATH=/usr/lib64:$LD_LIBRARY_PATH
