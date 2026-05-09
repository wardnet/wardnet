/* global React, WardData, Icon, Sparkline, Donut, Toggle, StatTile */
const { useState: useStateS, useEffect: useEffectS, useMemo: useMemoS, useRef: useRefS } = React;

/* ============ DASHBOARD ============ */
function DashboardScreen({ onToast }) {
  const [paused, setPaused] = useStateS(false);
  const [level, setLevel] = useStateS("INFO");
  const [logs, setLogs] = useStateS(WardData.initialLogs);
  const logRef = useRefS(null);

  useEffectS(() => {
    if (paused) return;
    const id = setInterval(() => {
      const samples = [
        { lvl: "info", m: "DNS query A " + ["graph.facebook.com","ssl.gstatic.com","apple.com","samsungcloud.com","cdn.cloudflare.com"][Math.floor(Math.random()*5)] + " → 192.168.100." + (10 + Math.floor(Math.random()*60)) },
        { lvl: "info", m: "wg_ward0 keepalive (rx " + (Math.random()*8|0) + "." + (Math.random()*9|0) + " KB)" },
        { lvl: "warn", m: "Specified IFLA_INET6_STATS NLA attribute holds more data than expected (288 → 304)" },
      ];
      const s = samples[Math.floor(Math.random() * samples.length)];
      const t = new Date();
      const tt = `${String(t.getHours()).padStart(2,"0")}:${String(t.getMinutes()).padStart(2,"0")}:${String(t.getSeconds()).padStart(2,"0")}.${String(t.getMilliseconds()).padStart(3,"0")}`;
      setLogs(prev => [{ t: tt, ...s }, ...prev].slice(0, 80));
    }, 2400);
    return () => clearInterval(id);
  }, [paused]);

  const filteredLogs = useMemoS(() => {
    if (level === "ALL") return logs;
    if (level === "WARN") return logs.filter(l => l.lvl !== "info");
    if (level === "ERR") return logs.filter(l => l.lvl === "err");
    return logs;
  }, [logs, level]);

  return (
    <div className="col gap-20" data-screen-label="01 Dashboard">
      <div className="row" style={{justifyContent:"space-between", alignItems:"flex-end"}}>
        <div>
          <h1 className="h-title">Dashboard</h1>
          <p className="h-sub">Live overview · home network · gateway 192.168.100.2</p>
        </div>
        <div className="row gap-8">
          <button className="btn btn--ghost"><Icon name="ext"/> Open API docs</button>
          <button className="btn btn--primary" onClick={() => onToast("Snapshot saved to /var/wardnet/snapshots/")}><Icon name="down"/> Snapshot</button>
        </div>
      </div>

      <div className="grid-3">
        <StatTile label="Devices" value="30" sub="2 new in last 24h" spark={WardData.sparkB}
          pill={<span className="pill pill--ok"><span className="dot" /> all reachable</span>} />
        <StatTile label="Tunnels" value="3" unit="/ 2 active"
          sub="Portugal · Portugal Dedicated · United States (down)"
          spark={WardData.sparkA}
          pill={<span className="pill pill--warn"><span className="dot" /> 1 down</span>} />
        <StatTile label="Uptime" value="4h 56m" sub="v2026.05.02 · last reboot 12:31" spark={WardData.sparkC} />

        <StatTile label="CPU" value="0.3" unit="%" bar={3} spark={WardData.sparkA} sparkColor="var(--info)" />
        <StatTile label="Memory" value="530.8" unit="MB" sub="of 7.9 GB" bar={7} spark={WardData.sparkB} sparkColor="var(--info)" />
        <StatTile label="DHCP" value="15" sub="active leases · 7% pool used" bar={7}
          pill={<span className="pill pill--ok"><span className="dot" /> running</span>} />
      </div>

      <div className="grid-2">
        <div className="card">
          <div className="row" style={{justifyContent:"space-between", marginBottom: 4}}>
            <div>
              <div className="stat__label" style={{padding:0}}>DNS queries · last 24h</div>
              <div className="stat__value" style={{marginTop:4}}>1,252,914</div>
              <div className="stat__sub">16 clients · 1,800 unique domains</div>
            </div>
            <div className="row gap-8">
              <span className="pill"><span className="dot" style={{background:"color-mix(in oklab, var(--accent) 60%, var(--bg-sunken))"}} /> Allowed</span>
              <span className="pill"><span className="dot" style={{background:"var(--danger)"}} /> Blocked</span>
            </div>
          </div>
          <div className="qchart">
            {WardData.dnsBars.map((b, i) => (
              <div key={i}>
                <span className="ok" style={{height:`${b.ok}%`}} />
                <span className="blk" style={{height:`${b.blk}%`}} />
              </div>
            ))}
          </div>
          <div className="qchart-axis">
            <span>−24h</span><span>−18h</span><span>−12h</span><span>−6h</span><span>−1h</span><span>now</span>
          </div>
        </div>

        <div className="card row gap-20" style={{alignItems:"center"}}>
          <Donut value={4747} max={1252914 * 0.05} label="0.4%" sub="blocked" />
          <div className="col gap-8" style={{flex:1}}>
            <div className="stat__label" style={{padding:0}}>Blocked traffic</div>
            <div style={{fontSize:13.5, color:"var(--ink-2)"}}>
              <strong style={{color:"var(--ink)"}}>4,747</strong> of 1,252,914 queries blocked in the last 24 hours.
            </div>
            <div className="col gap-8" style={{marginTop:6}}>
              {[
                {l:"Advertising", v:62},
                {l:"Trackers", v:24},
                {l:"Malware", v:9},
                {l:"Crypto miners", v:5},
              ].map(r => (
                <div key={r.l} className="row gap-12">
                  <div style={{fontSize:12, color:"var(--ink-3)", width:90}}>{r.l}</div>
                  <div className="bar" style={{flex:1}}><span style={{width:`${r.v}%`}}/></div>
                  <div className="mono" style={{fontSize:11, color:"var(--ink-3)", width:36, textAlign:"right"}}>{r.v}%</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div className="card card--flush">
        <div className="card__head">
          <h3>Live logs</h3>
          <span className="pill pill--ok"><span className="dot" /> {paused ? "paused" : "streaming"}</span>
          <div className="right">
            <div className="tabs">
              {["INFO","WARN","ERR","ALL"].map(l => (
                <button key={l} className={level === l ? "is-active" : ""} onClick={() => setLevel(l)}>{l}</button>
              ))}
            </div>
            <button className="btn btn--sm" onClick={() => setPaused(p => !p)}>
              <Icon name={paused ? "play" : "pause"} size={14} /> {paused ? "Resume" : "Pause"}
            </button>
            <button className="btn btn--sm" onClick={() => setLogs([])}><Icon name="trash" size={14} /> Clear</button>
            <button className="btn btn--sm" onClick={() => onToast("Logs exported")}><Icon name="down" size={14} /> Export</button>
          </div>
        </div>
        <div className="logs" ref={logRef}>
          {filteredLogs.map((l, i) => (
            <div key={i} className={"logrow is-" + l.lvl}>
              <div className="t">{l.t}</div>
              <div className="l">{l.lvl === "err" ? "ERROR" : l.lvl === "warn" ? "WARN" : "INFO"}</div>
              <div className="m">{l.m}</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ============ DEVICES ============ */
function DevicesScreen({ onOpen }) {
  const [q, setQ] = useStateS("");
  const [group, setGroup] = useStateS("All");
  const groups = ["All", ...Array.from(new Set(WardData.devices.map(d => d.group)))];
  const list = WardData.devices.filter(d =>
    (group === "All" || d.group === group) &&
    (q === "" || (d.name + " " + d.ip + " " + d.mac + " " + d.vendor).toLowerCase().includes(q.toLowerCase()))
  );
  const ico = { laptop:"laptop", phone:"phone", tv:"tv", router:"router", ap:"ap", console:"console", iot:"iot", printer:"printer", unknown:"unknown" };

  return (
    <div className="col gap-20" data-screen-label="02 Devices">
      <div className="row" style={{justifyContent:"space-between", alignItems:"flex-end"}}>
        <div>
          <h1 className="h-title">Devices</h1>
          <p className="h-sub">{list.length} of {WardData.devices.length} on the network · 5 groups</p>
        </div>
        <div className="row gap-8">
          <div className="topbar__search" style={{width:240}}>
            <Icon name="search" size={14} />
            <input placeholder="Search hostname, IP, MAC…" value={q} onChange={e => setQ(e.target.value)} />
          </div>
          <button className="btn btn--ghost"><Icon name="filter" size={14}/> Filter</button>
          <button className="btn btn--primary"><Icon name="plus" size={14}/> Add reservation</button>
        </div>
      </div>

      <div className="row wrap gap-8">
        {groups.map(g => (
          <button key={g}
            className={"pill " + (g === group ? "pill--ok" : "pill--ghost")}
            onClick={() => setGroup(g)}
            style={{cursor:"pointer", border:"1px solid var(--line)", padding:"5px 10px"}}>
            {g} <span className="mono" style={{opacity:0.7, marginLeft:4}}>
              {g === "All" ? WardData.devices.length : WardData.devices.filter(d => d.group === g).length}
            </span>
          </button>
        ))}
      </div>

      <div className="card card--flush">
        <table className="tbl">
          <thead>
            <tr>
              <th>Host</th><th>IP</th><th>Group</th><th>Throughput</th><th>Status</th><th></th>
            </tr>
          </thead>
          <tbody>
            {list.map(d => (
              <tr key={d.mac} style={{cursor:"pointer"}} onClick={() => onOpen && onOpen(d.mac)}>
                <td>
                  <div className="host">
                    <div className="avatar"><Icon name={ico[d.type] || "unknown"} size={15} /></div>
                    <div className="col">
                      <div className="name">{d.name}</div>
                      <div className="mac mono">{d.mac} · {d.vendor}</div>
                    </div>
                  </div>
                </td>
                <td className="mono" style={{color:"var(--ink-2)"}}>{d.ip}</td>
                <td><span className="pill pill--ghost">{d.group}</span></td>
                <td>
                  <div className="row gap-12 mono" style={{fontSize:12}}>
                    <span style={{color:"var(--accent-soft-ink)"}}>↓ {d.down} KB/s</span>
                    <span style={{color:"var(--info)"}}>↑ {d.up} KB/s</span>
                  </div>
                </td>
                <td>
                  <span className={"pill " + (d.status === "online" ? "pill--ok" : "pill--ghost")}>
                    <span className="dot" /> {d.status}
                  </span>
                </td>
                <td className="actions">
                  <a>Inspect</a><a>Block</a><a className="danger">Forget</a>
                </td>
              </tr>
            ))}
            {list.length === 0 && (
              <tr><td colSpan={6}><div className="empty">No devices match your filters.</div></td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/* ============ TUNNELS ============ */
function TunnelsScreen({ onToast, onAdd, onOpen }) {
  return (
    <div className="col gap-20" data-screen-label="03 Tunnels">
      <div className="row" style={{justifyContent:"space-between", alignItems:"flex-end"}}>
        <div>
          <h1 className="h-title">Tunnels</h1>
          <p className="h-sub">WireGuard peers · 2 active · 1 down</p>
        </div>
        <div className="row gap-8">
          <button className="btn btn--ghost"><Icon name="ext" size={14}/> Import config</button>
          <button className="btn btn--primary" onClick={onAdd}><Icon name="plus" size={14}/> Add tunnel</button>
        </div>
      </div>

      <div className="grid-2">
        {WardData.tunnels.map(t => {
          const bars = Array.from({length: 24}, (_, i) =>
            t.status === "down" ? 0 : Math.max(4, Math.round(40 + 30 * Math.sin(i / 2.5 + t.iface.length) + (i % 4) * 6))
          );
          return (
            <div className="tcard" key={t.iface} onClick={() => onOpen && onOpen(t.iface)} style={{cursor:"pointer"}}>
              <div className="tcard__head">
                <div className="tcard__flag" aria-hidden>{t.flag}</div>
                <div className="col" style={{flex:1}}>
                  <div className="tcard__title">{t.name}</div>
                  <div className="tcard__sub">{t.country} · {t.provider} · <span className="mono">{t.iface}</span></div>
                </div>
                <span className={"pill " + (t.status === "active" ? "pill--ok" : "pill--down")}>
                  <span className="dot" /> {t.status === "active" ? "Active" : "Down"}
                </span>
              </div>

              <dl className="tcard__grid">
                <div>
                  <dt>Endpoint</dt>
                  <dd>{t.endpoint}</dd>
                </div>
                <div>
                  <dt>Last handshake</dt>
                  <dd>{t.handshake}</dd>
                </div>
                <div>
                  <dt>Traffic</dt>
                  <dd>↑ {t.up} {t.upUnit}  ·  ↓ {t.down} {t.downUnit}</dd>
                </div>
                <div>
                  <dt>Allowed IPs</dt>
                  <dd>0.0.0.0/0, ::/0</dd>
                </div>
              </dl>

              <div className="tcard__throughput">
                {bars.map((h, i) => <span key={i} style={{height:`${h}%`}} />)}
              </div>

              <div className="row gap-8" style={{justifyContent:"flex-end"}}>
                <button className="btn btn--sm" onClick={() => onToast(`Copied ${t.iface}.conf`)}><Icon name="copy" size={13}/> Config</button>
                <button className="btn btn--sm" onClick={() => onToast(`${t.name}: ${t.status === "active" ? "stopped" : "starting"}`)}>
                  {t.status === "active" ? "Stop" : "Bring up"}
                </button>
                <button className="btn btn--sm btn--danger">Delete</button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ============ DHCP ============ */
function DhcpScreen({ onToast }) {
  const [enabled, setEnabled] = useStateS(true);
  const [tab, setTab] = useStateS("Leases");
  const ico = { laptop:"laptop", phone:"phone", tv:"tv", router:"router", ap:"ap", console:"console", iot:"iot", printer:"printer", unknown:"unknown" };

  return (
    <div className="col gap-20" data-screen-label="04 DHCP">
      <div className="row" style={{justifyContent:"space-between", alignItems:"flex-end"}}>
        <div>
          <h1 className="h-title">DHCP</h1>
          <p className="h-sub">192.168.100.0/24 · pool 50–250</p>
        </div>
      </div>

      <div className="grid-2">
        <div className="card">
          <div className="row" style={{justifyContent:"space-between"}}>
            <div className="row gap-8">
              <strong style={{fontSize:13}}>DHCP server</strong>
              <span className="pill pill--ok"><span className="dot" /> Running</span>
            </div>
            <Toggle on={enabled} onChange={v => { setEnabled(v); onToast(v ? "DHCP enabled" : "DHCP disabled"); }} />
          </div>
          <div className="grid-2" style={{marginTop:14}}>
            <div>
              <div className="stat__label" style={{padding:0}}>Active leases</div>
              <div className="stat__value" style={{marginTop:4}}>15</div>
            </div>
            <div>
              <div className="stat__label" style={{padding:0}}>Pool size</div>
              <div className="stat__value" style={{marginTop:4}}>201</div>
            </div>
          </div>
          <div style={{marginTop:14}}>
            <div className="row" style={{justifyContent:"space-between"}}>
              <div className="stat__label" style={{padding:0}}>Pool usage</div>
              <div className="mono" style={{fontSize:11, color:"var(--ink-3)"}}>15 / 201</div>
            </div>
            <div className="bar" style={{marginTop:6, height:6}}><span style={{width:"7.5%"}}/></div>
          </div>
        </div>

        <div className="card">
          <div className="row" style={{justifyContent:"space-between", marginBottom:12}}>
            <strong style={{fontSize:13}}>Configuration</strong>
            <button className="btn btn--sm btn--ghost">Edit</button>
          </div>
          <div className="grid-3" style={{rowGap:14}}>
            {[
              ["Gateway IP","192.168.100.2"],
              ["Pool range","192.168.100.50 — .250"],
              ["Subnet","255.255.255.0"],
              ["Lease duration","1d"],
              ["Fallback router","192.168.100.1"],
              ["Upstream DNS","192.168.100.2"],
            ].map(([k,v]) => (
              <div key={k}>
                <div className="stat__label" style={{padding:0}}>{k}</div>
                <div className="mono" style={{fontSize:12, color:"var(--ink)", marginTop:4}}>{v}</div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="row" style={{justifyContent:"space-between"}}>
        <div className="tabs">
          {["Leases","Reservations"].map(t => (
            <button key={t} className={t === tab ? "is-active" : ""} onClick={() => setTab(t)}>{t}</button>
          ))}
        </div>
        <button className="btn btn--sm btn--ghost"><Icon name="plus" size={13}/> New reservation</button>
      </div>

      <div className="card card--flush">
        <table className="tbl">
          <thead>
            <tr><th>Host</th><th>IP</th><th>Status</th><th>Expires</th><th></th></tr>
          </thead>
          <tbody>
            {WardData.devices.filter(d => d.ip.startsWith("192.168.100.")).slice(0, 12).map(d => (
              <tr key={d.mac}>
                <td>
                  <div className="host">
                    <div className="avatar"><Icon name={ico[d.type] || "unknown"} size={15}/></div>
                    <div className="col">
                      <div className="name">{d.name}</div>
                      <div className="mac mono">{d.mac}</div>
                    </div>
                  </div>
                </td>
                <td className="mono">{d.ip}</td>
                <td><span className={"pill " + (d.status === "online" ? "pill--ok" : "pill--ghost")}>{d.status === "online" ? "Active" : "Idle"}</span></td>
                <td className="muted">just now</td>
                <td className="actions">
                  <a onClick={() => onToast(`${d.name} pinned to ${d.ip}`)}>Make static</a>
                  <a className="danger" onClick={() => onToast(`Lease for ${d.name} revoked`)}>Revoke</a>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/* ============ DNS ============ */
function DnsScreen() {
  return (
    <div className="col gap-20" data-screen-label="05 DNS">
      <div>
        <h1 className="h-title">DNS</h1>
        <p className="h-sub">Resolver up · 192.168.100.2 · forwarding to 1.1.1.1, 9.9.9.9</p>
      </div>

      <div className="grid-3">
        <StatTile label="Queries / 24h" value="1,252,914" sub="14.5K avg per hour" spark={WardData.sparkB}/>
        <StatTile label="Cache hit rate" value="86" unit="%" sub="of 1.2M queries" bar={86}/>
        <StatTile label="Avg latency" value="6.4" unit="ms" sub="upstream Cloudflare" spark={WardData.sparkA} sparkColor="var(--info)"/>
      </div>

      <div className="grid-2">
        <div className="card">
          <div className="row" style={{justifyContent:"space-between", marginBottom:12}}>
            <strong style={{fontSize:13}}>Upstream resolvers</strong>
            <button className="btn btn--sm btn--ghost"><Icon name="plus" size={13}/> Add</button>
          </div>
          <div className="col gap-8">
            {[
              {n:"Cloudflare", a:"1.1.1.1", l:"4 ms", t:"DoH", on:true},
              {n:"Quad9", a:"9.9.9.9", l:"11 ms", t:"DoT", on:true},
              {n:"Google", a:"8.8.8.8", l:"7 ms", t:"Plain", on:false},
            ].map(r => (
              <div key={r.a} className="row" style={{justifyContent:"space-between", padding:"10px 12px", border:"1px solid var(--line)", borderRadius:10}}>
                <div className="col">
                  <div style={{fontSize:13.5, color:"var(--ink)"}}>{r.n} <span className="muted mono" style={{fontSize:12, marginLeft:6}}>{r.a}</span></div>
                  <div className="muted" style={{fontSize:12}}>{r.t} · {r.l}</div>
                </div>
                <Toggle on={r.on} onChange={()=>{}} />
              </div>
            ))}
          </div>
        </div>

        <div className="card">
          <div className="row" style={{justifyContent:"space-between", marginBottom:12}}>
            <strong style={{fontSize:13}}>Top domains · 24h</strong>
            <span className="muted" style={{fontSize:12}}>by query count</span>
          </div>
          <div className="col gap-8">
            {WardData.topDomains.map((d) => (
              <div key={d.d} className="row gap-12">
                <div className="mono" style={{fontSize:12.5, color:"var(--ink-2)", flex:1, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap"}}>{d.d}</div>
                <div className="bar" style={{flex:1, maxWidth:160}}><span style={{width:`${(d.q / 18421) * 100}%`, background: d.blocked ? "var(--danger)" : "var(--accent)"}}/></div>
                <div className="mono" style={{fontSize:11, color:"var(--ink-3)", width:64, textAlign:"right"}}>{d.q.toLocaleString()}</div>
                <span className={"pill " + (d.blocked ? "pill--down" : "pill--ghost")} style={{minWidth:64, justifyContent:"center"}}>
                  {d.blocked ? "Blocked" : "Allowed"}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ============ DNS FILTERING ============ */
function FilteringScreen({ onToast }) {
  const [cats, setCats] = useStateS(WardData.filteringCategories);
  const [allow, setAllow] = useStateS(["icloud.com","apple-mapkit.com"]);
  const [block, setBlock] = useStateS(["analytics.tiktok.com","ads.yahoo.com","fb.gg"]);
  const [newDomain, setNewDomain] = useStateS("");

  return (
    <div className="col gap-20" data-screen-label="06 DNS Filtering">
      <div>
        <h1 className="h-title">DNS Filtering</h1>
        <p className="h-sub">3 of 6 categories active · 4,747 blocked queries today</p>
      </div>

      <div className="card card--flush">
        <div className="card__head">
          <h3>Categories</h3>
          <div className="right">
            <span className="muted" style={{fontSize:12}}>{cats.filter(c=>c.on).length} on · {cats.filter(c=>!c.on).length} off</span>
          </div>
        </div>
        {cats.map(c => (
          <div className="cat" key={c.id}>
            <div className="cat__icon"><Icon name="shield2" size={16}/></div>
            <div className="col" style={{flex:1}}>
              <div className="cat__title">{c.name}</div>
              <div className="cat__desc">{c.desc}</div>
            </div>
            <div className="cat__count">{c.count.toLocaleString()} domains</div>
            <Toggle on={c.on} onChange={v => {
              setCats(p => p.map(x => x.id === c.id ? {...x, on:v} : x));
              onToast(`${c.name} ${v ? "enabled" : "disabled"}`);
            }}/>
          </div>
        ))}
      </div>

      <div className="grid-2">
        {[
          { title: "Custom blocklist", list: block, setter: setBlock, kind: "block" },
          { title: "Allowlist", list: allow, setter: setAllow, kind: "allow" },
        ].map(b => (
          <div className="card card--flush" key={b.title}>
            <div className="card__head">
              <h3>{b.title}</h3>
              <span className="muted" style={{fontSize:12}}>{b.list.length} entries</span>
            </div>
            <div style={{padding:14}}>
              <div className="row gap-8" style={{marginBottom:10}}>
                <input className="field" style={{flex:1, padding:"8px 10px", border:"1px solid var(--line)", borderRadius:8, background:"var(--bg-elev)", color:"var(--ink)", fontSize:13, fontFamily:"var(--font-mono)"}}
                  placeholder={b.kind === "block" ? "block.example.com" : "allow.example.com"}
                  onKeyDown={e => {
                    if (e.key === "Enter" && e.target.value) {
                      b.setter(p => [...p, e.target.value]);
                      onToast(`Added ${e.target.value}`);
                      e.target.value = "";
                    }
                  }}/>
                <button className="btn btn--sm">Add</button>
              </div>
              <div className="row wrap gap-8">
                {b.list.map(d => (
                  <span key={d} className={"pill " + (b.kind === "block" ? "pill--down" : "pill--ok")} style={{fontFamily:"var(--font-mono)"}}>
                    {d}
                    <Icon name="x" size={11}/>
                  </span>
                ))}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ============ SETTINGS ============ */
function SettingsScreen({ onToast }) {
  const [autoUpdate, setAutoUpdate] = useStateS(true);
  const [telemetry, setTelemetry] = useStateS(false);
  const [reboot, setReboot] = useStateS(false);

  return (
    <div className="col gap-20" data-screen-label="07 Settings">
      <div>
        <h1 className="h-title">Settings</h1>
        <p className="h-sub">System, updates, access</p>
      </div>

      <div className="card">
        <strong style={{fontSize:13, display:"block", marginBottom:14}}>System</strong>
        <div className="grid-3" style={{rowGap:14}}>
          {[
            ["Hostname","ward.local"],
            ["Version","v2026.05.02"],
            ["Channel","stable"],
            ["Architecture","aarch64-linux"],
            ["Uptime","4h 56m"],
            ["Last update","8 days ago"],
          ].map(([k,v]) => (
            <div key={k}>
              <div className="stat__label" style={{padding:0}}>{k}</div>
              <div className="mono" style={{fontSize:12, color:"var(--ink)", marginTop:4}}>{v}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="card">
        <strong style={{fontSize:13, display:"block", marginBottom:14}}>Preferences</strong>
        <div className="col gap-12">
          {[
            { k: "Auto-update firmware", d: "Apply patch releases automatically at 04:00.", v: autoUpdate, set: setAutoUpdate },
            { k: "Anonymous telemetry", d: "Help improve Wardnet by sharing aggregate stats.", v: telemetry, set: setTelemetry },
            { k: "Scheduled reboot", d: "Reboot weekly on Sunday at 03:00.", v: reboot, set: setReboot },
          ].map(p => (
            <div key={p.k} className="row" style={{justifyContent:"space-between", padding:"10px 0", borderTop:"1px solid var(--line)"}}>
              <div className="col">
                <div style={{fontSize:13.5, color:"var(--ink)"}}>{p.k}</div>
                <div className="muted" style={{fontSize:12}}>{p.d}</div>
              </div>
              <Toggle on={p.v} onChange={v => { p.set(v); onToast(`${p.k}: ${v ? "on" : "off"}`); }} />
            </div>
          ))}
        </div>
      </div>

      <div className="card">
        <strong style={{fontSize:13, display:"block", marginBottom:14}}>Danger zone</strong>
        <div className="col gap-8">
          <button className="btn btn--ghost" onClick={() => onToast("Restart scheduled in 30s")}>Restart services</button>
          <button className="btn btn--ghost" onClick={() => onToast("Reboot initiated")}>Reboot device</button>
          <button className="btn btn--danger" onClick={() => onToast("Factory reset cancelled")}>Factory reset…</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { DashboardScreen, DevicesScreen, TunnelsScreen, DhcpScreen, DnsScreen, FilteringScreen, SettingsScreen });
