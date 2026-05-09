/* global React, WardData, Icon, Sparkline, Toggle */
const { useState: useS_d, useMemo: useM_d } = React;

/* ========== TUNNEL DETAIL ========== */
function TunnelDetailScreen({ id, onBack, onToast }) {
  const t = WardData.tunnels.find(x => x.iface === id) || WardData.tunnels[0];
  const [range, setRange] = useS_d("24h");
  const ranges = ["1h","6h","24h","48h","12mo"];
  const points = useM_d(() => Array.from({length: 96}, (_, i) => {
    if (t.status === "down") return {d:0, u:0};
    const base = 200 + 280 * Math.abs(Math.sin(i / 7 + t.iface.length));
    const spike = (i % 18 === 0) ? 800 : (i % 23 === 0) ? 600 : 0;
    return {
      d: Math.max(8, Math.round(base + spike + (i % 5) * 30)),
      u: Math.max(2, Math.round(base / 6 + (spike ? spike / 8 : 0))),
    };
  }), [t.iface, t.status, range]);

  const peers = WardData.devices.filter(d => ["192.168.100.55","192.168.100.62"].includes(d.ip));

  return (
    <div className="col gap-20" data-screen-label="Tunnel detail">
      <div className="row" style={{justifyContent:"space-between", alignItems:"flex-start"}}>
        <div className="col gap-8">
          <div className="row gap-8 muted" style={{fontSize:13}}>
            <a onClick={onBack} style={{cursor:"pointer", color:"var(--ink-3)"}}>Tunnels</a>
            <span>›</span>
            <span>{t.name}</span>
          </div>
          <div className="row gap-12" style={{alignItems:"center"}}>
            <span style={{fontSize:26}}>{t.flag}</span>
            <h1 className="h-title">{t.name}</h1>
            <span className={"pill " + (t.status === "active" ? "pill--ok" : "pill--down")}><span className="dot"/> {t.status === "active" ? "Active" : "Down"}</span>
          </div>
          <div className="muted" style={{fontSize:13}}>Last handshake: {t.handshake}</div>
        </div>
        <div className="row gap-8">
          <button className="btn btn--sm" onClick={() => onToast(`Copied ${t.iface}.conf`)}><Icon name="copy" size={13}/> Config</button>
          <button className="btn btn--sm" onClick={() => onToast(`${t.name} ${t.status === "active" ? "stopped" : "starting"}`)}>{t.status === "active" ? "Stop" : "Bring up"}</button>
        </div>
      </div>

      <div className="card">
        <strong style={{fontSize:13, display:"block", marginBottom:14}}>Configuration</strong>
        <div className="grid-4" style={{rowGap:14}}>
          {[
            ["Provider", t.provider.toLowerCase()],
            ["Country", `${t.flag} ${t.country}`],
            ["Endpoint", t.endpoint],
            ["Interface", t.iface],
          ].map(([k,v]) => (
            <div key={k}>
              <div className="stat__label" style={{padding:0}}>{k}</div>
              <div className="mono" style={{fontSize:12, color:"var(--ink)", marginTop:4}}>{v}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="card">
        <div className="row" style={{justifyContent:"space-between", marginBottom: 6}}>
          <div className="col">
            <strong style={{fontSize:13}}>Throughput</strong>
            <div className="muted" style={{fontSize:12, marginTop:2}}>Window total: <span className="mono" style={{color:"var(--ink-2)"}}>↓ 3.1 GB · ↑ 186.7 MB</span></div>
          </div>
          <div className="tabs">
            {ranges.map(r => (
              <button key={r} className={r === range ? "is-active" : ""} onClick={() => setRange(r)}>{r}</button>
            ))}
          </div>
        </div>
        <ThroughputChart points={points} />
        <div className="row gap-16" style={{justifyContent:"center", marginTop:8}}>
          <span className="row gap-8" style={{fontSize:12, color:"var(--ink-3)"}}><span style={{display:"inline-block", width:18, height:2, background:"var(--accent)"}}/> Download</span>
          <span className="row gap-8" style={{fontSize:12, color:"var(--ink-3)"}}><span style={{display:"inline-block", width:18, height:2, background:"var(--info)"}}/> Upload</span>
        </div>
      </div>

      <div className="card card--flush">
        <div className="card__head">
          <h3>Devices using this tunnel</h3>
          <span className="muted" style={{fontSize:12}}>({peers.length})</span>
        </div>
        <table className="tbl">
          <thead><tr><th>Device</th><th>IP</th><th></th></tr></thead>
          <tbody>
            {peers.map(d => (
              <tr key={d.mac}>
                <td>
                  <div className="host">
                    <div className="avatar"><Icon name={d.type === "phone" ? "phone" : d.type === "tv" ? "tv" : "iot"} size={15}/></div>
                    <div className="col">
                      <div className="name">{d.name}</div>
                      <div className="mac mono">{d.mac.toUpperCase()}</div>
                    </div>
                  </div>
                </td>
                <td className="mono">{d.ip}</td>
                <td className="actions"><a>Remove</a></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="row" style={{justifyContent:"flex-end"}}>
        <button className="btn btn--danger">Delete tunnel</button>
      </div>
    </div>
  );
}

function ThroughputChart({ points }) {
  const w = 1100, h = 220, p = { l: 56, r: 12, t: 10, b: 26 };
  const max = Math.max(1, ...points.map(p => Math.max(p.d, p.u)));
  const yScale = (v) => h - p.b - (v / max) * (h - p.t - p.b);
  const xScale = (i) => p.l + (i / (points.length - 1)) * (w - p.l - p.r);
  const path = (key) => points.map((pt, i) => `${i ? "L" : "M"}${xScale(i).toFixed(1)} ${yScale(pt[key]).toFixed(1)}`).join(" ");
  const area = (key) => `${path(key)} L ${xScale(points.length-1)} ${h - p.b} L ${p.l} ${h - p.b} Z`;
  const ticks = [0, 0.25, 0.5, 0.75, 1].map(f => Math.round(max * f));
  return (
    <svg viewBox={`0 0 ${w} ${h}`} style={{width:"100%", height: 220}}>
      {/* gridlines */}
      {ticks.map((v, i) => {
        const y = yScale(v);
        return (
          <g key={i}>
            <line x1={p.l} y1={y} x2={w - p.r} y2={y} stroke="var(--line)" strokeDasharray={i === 0 ? "0" : "2 4"} />
            <text x={p.l - 8} y={y + 3} textAnchor="end" fontSize="10" fontFamily="var(--font-mono)" fill="var(--ink-4)">{v >= 1000 ? (v/1024).toFixed(1) + " MB/s" : v + " KB/s"}</text>
          </g>
        );
      })}
      {/* areas */}
      <path d={area("d")} fill="color-mix(in oklab, var(--accent) 14%, transparent)"/>
      <path d={area("u")} fill="color-mix(in oklab, var(--info) 12%, transparent)"/>
      <path d={path("d")} fill="none" stroke="var(--accent)" strokeWidth="1.6"/>
      <path d={path("u")} fill="none" stroke="var(--info)" strokeWidth="1.6"/>
      {/* x labels */}
      {[0, 0.25, 0.5, 0.75, 1].map((f, i) => {
        const lbls = ["−24h","−18h","−12h","−6h","now"];
        return <text key={i} x={xScale(Math.floor(f * (points.length-1)))} y={h - 6} fontSize="10" fontFamily="var(--font-mono)" fill="var(--ink-4)" textAnchor="middle">{lbls[i]}</text>;
      })}
    </svg>
  );
}

/* ========== DEVICE DETAIL ========== */
function DeviceDetailScreen({ mac, onBack, onToast }) {
  const d = WardData.devices.find(x => x.mac === mac) || WardData.devices[3];
  const [editing, setEditing] = useS_d(false);
  const [form, setForm] = useS_d({ name: d.name, type: d.type, routing: "Direct (no VPN)", lock: false });
  const [routingOpen, setRoutingOpen] = useS_d(false);

  const ico = { laptop:"laptop", phone:"phone", tv:"tv", router:"router", ap:"ap", console:"console", iot:"iot", printer:"printer", unknown:"unknown" };
  const routingOptions = [
    { v: "Direct (no VPN)", icon: null },
    ...WardData.tunnels.map(t => ({ v: t.name, icon: t.flag })),
  ];

  return (
    <div className="col gap-20" data-screen-label="Device detail">
      <div className="col gap-8">
        <div className="row gap-8 muted" style={{fontSize:13}}>
          <a onClick={onBack} style={{cursor:"pointer", color:"var(--ink-3)"}}>Devices</a>
          <span>›</span>
          <span>{d.name}</span>
        </div>
        <div className="row gap-12" style={{alignItems:"center"}}>
          <Icon name={ico[d.type] || "unknown"} size={22}/>
          <h1 className="h-title">{form.name}</h1>
          <span className={"pill " + (d.status === "online" ? "pill--ok" : "pill--ghost")}><span className="dot"/> {d.status === "online" ? "Online" : "Idle"}</span>
        </div>
        <div className="muted" style={{fontSize:13}}>Last seen: just now · {form.type[0].toUpperCase() + form.type.slice(1)} · <span className="mono">{d.ip}</span></div>
      </div>

      <div className="card">
        <strong style={{fontSize:13, display:"block", marginBottom:14}}>Identity</strong>
        <div className="grid-3" style={{rowGap:18}}>
          {[
            ["MAC", d.mac.toUpperCase(), true],
            ["Hostname", d.name.toLowerCase().replace(/\s+/g,"-")+"-"+d.type, false],
            ["Manufacturer", d.vendor === "—" ? "Randomized MAC" : d.vendor, false],
            ["First seen", "23d ago", false],
            ["Last seen", "just now", false],
            ["OS", d.os, false],
          ].map(([k,v,mono]) => (
            <div key={k}>
              <div className="stat__label" style={{padding:0}}>{k}</div>
              <div className={mono ? "mono" : ""} style={{fontSize: mono ? 12 : 13.5, color:"var(--ink)", marginTop:4}}>{v}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="card" style={{position:"relative"}}>
        <div className="row" style={{justifyContent:"space-between", marginBottom:14}}>
          <strong style={{fontSize:13}}>Settings</strong>
          {!editing && <button className="btn btn--sm" onClick={() => setEditing(true)}>Edit</button>}
        </div>

        {!editing ? (
          <div className="grid-2" style={{rowGap:18}}>
            <div>
              <div className="stat__label" style={{padding:0}}>Friendly name</div>
              <div style={{fontSize:13.5, color:"var(--ink)", marginTop:4}}>{form.name}</div>
            </div>
            <div>
              <div className="stat__label" style={{padding:0}}>Type</div>
              <div className="row gap-8" style={{marginTop:4}}>
                <Icon name={ico[form.type]} size={14}/>
                <span style={{fontSize:13.5, color:"var(--ink)", textTransform:"capitalize"}}>{form.type}</span>
              </div>
            </div>
            <div>
              <div className="stat__label" style={{padding:0}}>Routing</div>
              <div style={{fontSize:13.5, color:"var(--ink)", marginTop:4}}>{form.routing}</div>
            </div>
            <div>
              <div className="stat__label" style={{padding:0}}>Admin lock</div>
              <div style={{fontSize:13.5, color:"var(--ink)", marginTop:4}}>{form.lock ? "Locked" : "Unlocked"}</div>
            </div>
          </div>
        ) : (
          <div className="grid-2" style={{rowGap:14, columnGap:24}}>
            <div className="field">
              <label>Friendly name</label>
              <input value={form.name} onChange={e => setForm(f => ({...f, name: e.target.value}))} />
            </div>
            <div className="field">
              <label>Device type</label>
              <select value={form.type} onChange={e => setForm(f => ({...f, type: e.target.value}))}>
                {Object.keys(ico).map(k => <option key={k} value={k}>{k[0].toUpperCase() + k.slice(1)}</option>)}
              </select>
            </div>
            <div className="field" style={{position:"relative"}}>
              <label>Routing</label>
              <div className="dropdown" onClick={() => setRoutingOpen(o => !o)}
                   style={{padding:"8px 10px", border:"1px solid var(--line)", borderRadius:8, background:"var(--bg-elev)", fontSize:13, cursor:"pointer", display:"flex", alignItems:"center", gap:8}}>
                {form.routing === "Direct (no VPN)" ? <Icon name="x" size={14}/> : <span>{routingOptions.find(o => o.v === form.routing)?.icon}</span>}
                <span style={{flex:1}}>{form.routing}</span>
                <Icon name="down" size={12}/>
              </div>
              {routingOpen && (
                <div style={{position:"absolute", top: 60, left: 0, right: 0, background:"var(--bg-card)", border:"1px solid var(--line)", borderRadius:10, boxShadow:"var(--shadow-pop)", zIndex:10, overflow:"hidden"}}>
                  {routingOptions.map(o => (
                    <div key={o.v}
                      onClick={() => { setForm(f => ({...f, routing: o.v})); setRoutingOpen(false); }}
                      style={{padding:"10px 12px", display:"flex", alignItems:"center", gap:10, cursor:"pointer", fontSize:13, background: o.v === form.routing ? "var(--accent-soft)" : "transparent"}}>
                      {o.icon ? <span style={{fontSize:16}}>{o.icon}</span> : <Icon name="x" size={14}/>}
                      <span style={{flex:1}}>{o.v}</span>
                      {o.v === form.routing && <Icon name="check" size={14}/>}
                    </div>
                  ))}
                </div>
              )}
            </div>
            <div className="field">
              <label>Admin lock</label>
              <div className="row" style={{justifyContent:"space-between", padding:"8px 10px", border:"1px solid var(--line)", borderRadius:8, background:"var(--bg-elev)"}}>
                <span style={{fontSize:13, color:"var(--ink-3)"}}>Prevent user routing changes</span>
                <Toggle on={form.lock} onChange={v => setForm(f => ({...f, lock: v}))}/>
              </div>
            </div>
          </div>
        )}

        {editing && (
          <div className="row gap-8" style={{justifyContent:"flex-end", marginTop:14}}>
            <button className="btn btn--ghost" onClick={() => setEditing(false)}>Cancel</button>
            <button className="btn btn--primary" onClick={() => { setEditing(false); onToast("Device settings saved"); }}>Save</button>
          </div>
        )}
      </div>

      <div className="card">
        <div className="row" style={{justifyContent:"space-between", marginBottom:14}}>
          <strong style={{fontSize:13}}>DNS filtering</strong>
          <button className="btn btn--sm">Edit</button>
        </div>
        <div className="grid-2" style={{rowGap:18}}>
          <div>
            <div className="stat__label" style={{padding:0}}>Status</div>
            <div style={{fontSize:13.5, color:"var(--ink)", marginTop:4}}>Filtering on</div>
          </div>
          <div>
            <div className="stat__label" style={{padding:0}}>Profiles</div>
            <div style={{fontSize:13.5, color:"var(--ink)", marginTop:4}}>Ad Blocking <span className="muted">(default)</span></div>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="row" style={{justifyContent:"space-between", marginBottom:14}}>
          <strong style={{fontSize:13}}>Network</strong>
          <button className="btn btn--sm">Edit</button>
        </div>
        <div className="grid-2" style={{rowGap:18}}>
          <div>
            <div className="stat__label" style={{padding:0}}>IP</div>
            <div className="mono" style={{fontSize:12, color:"var(--ink)", marginTop:4}}>{d.ip}</div>
          </div>
          <div>
            <div className="stat__label" style={{padding:0}}>DHCP</div>
            <div style={{marginTop:4}}><span className="pill pill--ok">Lease</span></div>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { TunnelDetailScreen, DeviceDetailScreen });
