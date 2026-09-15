document.addEventListener("DOMContentLoaded", () => {
  const wsProto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${wsProto}//${location.host}/ws`;
  
  let ws = null;
  const historySize = 300;
  
  let minPower = sessionStorage.getItem("minPower") ? parseFloat(sessionStorage.getItem("minPower")) : Infinity;
  let maxPower = sessionStorage.getItem("maxPower") ? parseFloat(sessionStorage.getItem("maxPower")) : 0;
  
  const minPowerEl = document.getElementById("minPower");
  const maxPowerEl = document.getElementById("maxPower");
  if (minPowerEl && minPower !== Infinity) minPowerEl.textContent = minPower.toFixed(3);
  if (maxPowerEl && maxPower !== 0) maxPowerEl.textContent = maxPower.toFixed(3);

  const colors = {
    green: "#10b981",
    yellow: "#eab308",
    orange: "#f97316",
    red: "#ef4444",
    teal: "#14b8a6",
    grid: "#2a2a2a",
    text: "#9ca3af"
  };

  const getSize = (id) => {
    const el = document.getElementById(id);
    let w = el.clientWidth || 300;
    let h = el.clientHeight || 160;
    return { width: w, height: h };
  };

  const commonOpts = (id, label, color, min = 0, max = null) => {
    return {
      ...getSize(id),
      class: "uplot-modern",
      cursor: { points: { show: false }, drag: { setScale: false } },
      select: { show: false },
      legend: { show: true },
      scales: {
        x: { time: true },
        y: { range: (u, dMin, dMax) => [min, max !== null ? max : dMax * 1.1] }
      },
      axes: [
        { show: false }, 
        { 
          stroke: colors.text, 
          font: "10px 'Geist Mono', monospace",
          grid: { stroke: colors.grid, width: 1 },
          ticks: { show: false }
        }
      ],
      series: [
        {
          value: (u, v) => v == null ? "-" : new Date(v * 1000).toLocaleTimeString()
        },
        {
          label: label,
          stroke: color,
          fill: color + "33",
          width: 2,
          value: (u, v) => v == null ? "-" : v.toFixed(1)
        }
      ]
    };
  };

  const multiOpts = (id, labels, seriesColors) => {
    return {
      ...getSize(id),
      class: "uplot-modern",
      cursor: { points: { show: false }, drag: { setScale: false } },
      select: { show: false },
      legend: { show: true },
      scales: {
        x: { time: true },
        y: { range: (u, min, max) => [0, max * 1.1] }
      },
      axes: [
        { show: false },
        { 
          stroke: colors.text, 
          font: "10px 'Geist Mono', monospace",
          grid: { stroke: colors.grid, width: 1 },
          ticks: { show: false }
        }
      ],
      series: [
        {
          value: (u, v) => v == null ? "-" : new Date(v * 1000).toLocaleTimeString()
        },
        ...labels.map((l, i) => ({
          label: l,
          stroke: seriesColors[i],
          fill: seriesColors[i] + "33",
          width: 2,
          value: (u, v) => v == null ? "-" : v.toFixed(1)
        }))
      ]
    };
  };

  const charts = {
    cpu: new uPlot(commonOpts("cpuChart", "CPU", colors.green, 0, 100), [[], []], document.getElementById("cpuChart")),
    mem: new uPlot(commonOpts("memChart", "Memory", colors.orange, 0, 100), [[], []], document.getElementById("memChart")),
    temp: new uPlot(commonOpts("tempChart", "Temp", colors.red, 0, 100), [[], []], document.getElementById("tempChart")),
    gpuFreq: new uPlot(commonOpts("gpuFreqChart", "GPU", colors.yellow, 0, 1500), [[], []], document.getElementById("gpuFreqChart")),
    fan: new uPlot(commonOpts("fanChart", "Fan", colors.teal, 0, 8000), [[], []], document.getElementById("fanChart")),
    cpuFreq: new uPlot(commonOpts("freqChart", "CPU MHz", colors.green, 0, 3000), [[], []], document.getElementById("freqChart")),
    diskIO: new uPlot(multiOpts("diskIOChart", ["Read", "Write"], [colors.teal, colors.red]), [[], [], []], document.getElementById("diskIOChart")),
    netIO: new uPlot(multiOpts("netIOChart", ["Recv", "Send"], [colors.teal, colors.yellow]), [[], [], []], document.getElementById("netIOChart")),
  };

  const powerChart = new uPlot(commonOpts("powerChart", "Total Power", colors.green, 0), [[], []], document.getElementById("powerChart"));

  window.addEventListener("resize", () => {
    for (const [id, chart] of Object.entries(charts)) {
      chart.setSize(getSize(chart.root.parentElement.id));
    }
    powerChart.setSize(getSize("powerChart"));
  });

  let t = [];
  let d_cpu = []; let d_mem = []; let d_temp = []; let d_gpu = []; let d_fan = []; let d_freq = []; let d_disk = [];
  let d_disk_r = []; let d_disk_w = []; let d_net_r = []; let d_net_s = [];
  
  let pt = []; let p_val = [];

  function updateDOM(stats) {
    document.getElementById("cpuStat").textContent = `${(stats.cpu * 100).toFixed(1)}%`;
    document.getElementById("memStat").textContent = `${(stats.memory_percent * 100).toFixed(1)}%`;
    
    const tempEl = document.getElementById("tempStat");
    tempEl.textContent = `${stats.temp.toFixed(1)}°C`;
    tempEl.className = "value"; 
    if (stats.temp < 55) {
      tempEl.classList.add("text-green");
    } else if (stats.temp < 65) {
      tempEl.classList.add("text-yellow");
    } else {
      tempEl.classList.add("text-red");
    }

    document.getElementById("gpuFreqStat").textContent = `${stats.gpu_freq} MHz`;
    document.getElementById("fanSpeedLabel").textContent = `${stats.fan_rpm} RPM`;
    document.getElementById("freqStat").textContent = `${stats.cpu_freq.toFixed(0)} MHz`;
    document.getElementById("diskStat").textContent = `${(stats.disk_usage_percent * 100).toFixed(1)}%`;
    
    document.getElementById("diskReadStat").textContent = `${stats.disk_read_mb_s.toFixed(1)}`;
    document.getElementById("diskWriteStat").textContent = `${stats.disk_write_mb_s.toFixed(1)}`;
    document.getElementById("netRecvStat").textContent = `${stats.net_recv_mb_s.toFixed(1)}`;
    document.getElementById("netSentStat").textContent = `${stats.net_sent_mb_s.toFixed(1)}`;

    document.getElementById("uptime").textContent = stats.uptime_human;
    document.getElementById("loadAvg").textContent = stats.load_avg;
    
    const ethListEl = document.getElementById("ethList");
    if (ethListEl) {
      if (stats.eth_interfaces && stats.eth_interfaces.length > 0) {
        ethListEl.innerHTML = stats.eth_interfaces.map(eth => `
          <div class="info-item row">
            <span class="info-label">Ethernet (${eth.name})</span>
            <div class="info-value" style="text-align: right;">
              <div class="${eth.status === "UP" ? "text-green" : "text-red"}">${eth.status}</div>
              <div style="font-size: 0.8rem; color: var(--text-muted);">${eth.speed}</div>
              <div style="font-size: 0.8rem; color: var(--text-muted);">${eth.ip}</div>
            </div>
          </div>
        `).join("");
      } else {
        const ethName = stats.eth_name || "eth0";
        ethListEl.innerHTML = `
          <div class="info-item row">
            <span class="info-label">Ethernet (${ethName})</span>
            <div class="info-value" style="text-align: right;">
              <div class="${stats.eth_status === "UP" ? "text-green" : "text-red"}">${stats.eth_status || "DOWN"}</div>
              <div style="font-size: 0.8rem; color: var(--text-muted);">${stats.eth_speed || "--"}</div>
              <div style="font-size: 0.8rem; color: var(--text-muted);">${stats.eth_ip || "--"}</div>
            </div>
          </div>
        `;
      }
    }

    const wifiContainer = document.getElementById("wifiContainer");
    if (wifiContainer) {
      const wifi = stats.wifi;
      if (wifi && wifi.status === "UP") {
        wifiContainer.style.display = "flex";
        const wifiLabel = document.getElementById("wifiLabel");
        if (wifiLabel) wifiLabel.textContent = `Wi-Fi (${wifi.name})`;
        const wifiEl = document.getElementById("wifiStatus");
        if (wifiEl) {
          wifiEl.textContent = wifi.status;
          wifiEl.className = "text-green";
          document.getElementById("wifiSignal").textContent = wifi.signal;
          document.getElementById("wifiFreq").textContent = wifi.freq;
          document.getElementById("wifiSec").textContent = wifi.sec;
          document.getElementById("wifiIp").textContent = wifi.ip;
        }
      } else {
        wifiContainer.style.display = "none";
      }
    }

    const badge = document.getElementById("statusBadge");
    if (badge) {
      const p = stats.cpu_pressure_1m;
      badge.className = "status-indicator";
      if (p >= 70 && p < 100) {
        badge.classList.add("busy");
      } else if (p >= 100) {
        badge.classList.add("overloaded");
      }
    }
  }

  function appendData(stats) {
    const time = stats.timestamp / 1000;
    t.push(time);
    d_cpu.push(stats.cpu * 100);
    d_mem.push(stats.memory_percent * 100);
    d_temp.push(stats.temp);
    d_gpu.push(stats.gpu_freq);
    d_fan.push(stats.fan_rpm);
    d_freq.push(stats.cpu_freq);
    d_disk.push(stats.disk_usage_percent * 100);
    d_disk_r.push(stats.disk_read_mb_s);
    d_disk_w.push(stats.disk_write_mb_s);
    d_net_r.push(stats.net_recv_mb_s);
    d_net_s.push(stats.net_sent_mb_s);

    if (t.length > historySize) {
      t.shift(); d_cpu.shift(); d_mem.shift(); d_temp.shift(); d_gpu.shift(); d_fan.shift(); d_freq.shift(); d_disk.shift();
      d_disk_r.shift(); d_disk_w.shift(); d_net_r.shift(); d_net_s.shift();
    }
  }

  function renderCharts() {
    charts.cpu.setData([t, d_cpu]);
    charts.mem.setData([t, d_mem]);
    charts.temp.setData([t, d_temp]);
    charts.gpuFreq.setData([t, d_gpu]);
    charts.fan.setData([t, d_fan]);
    charts.cpuFreq.setData([t, d_freq]);
    charts.diskIO.setData([t, d_disk_r, d_disk_w]);
    charts.netIO.setData([t, d_net_r, d_net_s]);
  }

  function connectWs() {
    ws = new WebSocket(wsUrl);
    
    ws.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      if (msg.type === "history") {
        if (msg.info) {
          document.getElementById("hostName").textContent = msg.info.hostname;
          document.getElementById("osName").textContent = msg.info.os;
        }

        const hist = msg.data;
        t = []; d_cpu = []; d_mem = []; d_temp = []; d_gpu = []; d_fan = []; d_freq = []; d_disk = [];
        d_disk_r = []; d_disk_w = []; d_net_r = []; d_net_s = [];
        
        hist.forEach(stats => appendData(stats));
        renderCharts();
        
        if (hist.length > 0) {
          updateDOM(hist[hist.length - 1]);
        }
      } else if (msg.type === "update") {
        appendData(msg.data);
        renderCharts();
        updateDOM(msg.data);
      }
    };

    ws.onclose = () => {
      const badge = document.getElementById("statusBadge");
      if(badge) badge.className = "status-indicator overloaded";
      setTimeout(connectWs, 2000);
    };
  }

  connectWs();

  async function updatePower() {
    try {
      const res = await fetch("/api/power");
      const data = await res.json();
      
      const now = Date.now() / 1000;
      pt.push(now);
      p_val.push(data.total_power);

      if (pt.length > historySize) {
        pt.shift();
        p_val.shift();
      }

      powerChart.setData([pt, p_val]);
      document.getElementById("totalPowerStat").textContent = `${data.total_power.toFixed(3)} W`;

      if (data.total_power < minPower) {
        minPower = data.total_power;
        sessionStorage.setItem("minPower", minPower);
        minPowerEl.textContent = minPower.toFixed(3);
      }
      if (data.total_power > maxPower) {
        maxPower = data.total_power;
        sessionStorage.setItem("maxPower", maxPower);
        maxPowerEl.textContent = maxPower.toFixed(3);
      }

      const grid = document.getElementById("powerRailsGrid");
      if (grid) {
        if (data.readings && data.readings.length > 0) {
          grid.innerHTML = data.readings.map(r => `
            <div class="power-rail-card">
              <div class="rail-header">
                <span class="rail-name" title="${r.description}">${r.name}</span>
                <span class="rail-tag">${r.rail}</span>
              </div>
              <div class="rail-power">${r.power.toFixed(3)} W</div>
              <div class="rail-details">${r.voltage.toFixed(2)} V &bull; ${r.current.toFixed(3)} A</div>
            </div>
          `).join("");
        } else {
          grid.innerHTML = '<div style="color: var(--text-muted); font-size: 0.82rem; grid-column: 1 / -1;">No PMIC rail data available (verify /dev/vcio device mapping).</div>';
        }
      }
    } catch (err) {
      console.error("Power API failed:", err);
    }
  }

  setInterval(updatePower, 2000);
  updatePower();
});
