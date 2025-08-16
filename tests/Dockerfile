# Dockerfile for running Rtain tests in a consistent Linux environment

FROM rust:1.75-slim-bullseye

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    iptables \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy source code
COPY . .

# Build dependencies first for better caching
RUN cargo fetch

# Run tests
CMD ["make", "test"]