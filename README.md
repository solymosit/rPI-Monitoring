# rPI-Monitoring

A lightweight, real-time web monitor for Raspberry Pi built with Rust (Axum) and uPlot. It provides a dark-themed, low-overhead dashboard for tracking system resources, networking, and onboard PMIC power metrics on the Raspberry Pi 5.

## Features

- **System Metrics (1 Hz via WebSocket)**: CPU usage, core frequencies, memory consumption, load averages, and thermal zone temperatures.
- **Cooling**: Real-time PWM fan speed (RPM) read from `hwmon`.
- **I/O & Storage**: Disk space usage and read/write throughput from `/proc/diskstats`.
- **Networking**: Per-interface detection and status for Ethernet (`eth*`, `en*`) and Wi-Fi (`wlan*`), including IP addresses, link speed, and wireless signal strength.
- **Power Telemetry (Raspberry Pi 5)**: Real-time PMIC rail breakdown via `vcgencmd pmic_read_adc`, reporting voltage, current, and wattage across individual subsystems:
  - CPU & GPU Cores (`VDD_CORE`)
  - 3.3V System & GPIO (`3V3_SYS`)
  - LPDDR4X Memory (`DDR_VDD2`, `DDR_VDDQ`)
  - PCIe & USB (`1V1_SYS`)
  - RP1 Southbridge I/O Controller (`0V8_SW`)
  - Wi-Fi / Bluetooth, HDMI, Audio DAC, and ADC rails

## Quick Start

The easiest way to run the dashboard is with Docker Compose.

```yaml
services:
  pi-monitor:
    image: egyiptomi/rpi-monitoring:latest
    container_name: pi-monitor
    restart: unless-stopped
    volumes:
      - /etc/os-release:/etc/os-release:ro
      - /etc/hostname:/etc/hostname:ro
      - /proc:/proc:ro
      - /sys:/sys:ro
      - /var/run/wpa_supplicant:/var/run/wpa_supplicant:ro
    devices:
      - /dev/vcio:/dev/vcio
    privileged: true
    network_mode: "host"
    uts: "host"
```

## Building manually:


### 1. Clone the repository

```bash
git clone https://github.com/solymosit/rPI-Monitoring.git
cd rPI-Monitoring
```

### 2. Run with Docker Compose

```bash
docker compose up -d --build
```

Access the dashboard in your browser at `http://<your-pi-ip>:5000`.

## Architecture

- **Backend**: Rust using Tokio and Axum. A background task samples system metrics once per second and broadcasts JSON frames over WebSocket (`/ws`) to all connected browser clients. An in-memory ring buffer keeps the last 5 minutes of historical data to populate charts on client connect.
- **Power Endpoint**: `/api/power` is polled independently (every 2 seconds) by the frontend to invoke `vcgencmd pmic_read_adc`. This isolates hardware I2C firmware queries from the core system WebSocket loop.
- **Frontend**: Vanilla JavaScript and CSS with [uPlot](https://github.com/leeoniya/uPlot) for high-performance canvas chart rendering. Assets are served directly by Axum.
