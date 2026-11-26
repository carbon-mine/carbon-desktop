/** An alpine cirque at blue hour. The host supplies the lake surface at Y=0. */
export function buildAlpine(box) {
  let state=52271;
  const random=()=>((state=(Math.imul(state,1664525)+1013904223)>>>0)/4294967296);
  const pick=palette=>palette[Math.floor(random()*palette.length)];
  const B=box;
  const bare=['#4a596c','#526175','#586779','#5b6c81','#536479'];
  const icy=['#879bad','#92a4b6','#9aafbf','#aabcce','#a4b9c9'];
  const snow=['#b0c3d2','#bacbda','#a8bfd3','#bacdde','#b0c6da'];
  const bank=['#6f869a','#778fa3','#869db1','#97aabd','#7c92a6'];
  const pine=['#29424d','#304b53','#395159','#344e58','#3b5660'];
  const palePine=['#8ea9bd','#9bb4c7','#7f9bac','#acc2cf'];
  const distanceSnow=['#879bb0','#90a6b9','#8fa3bb','#a1b2c6','#9cafc3'];

  function crown(x,z,amplitude,wide,deep,face=0) {
    return {x,z,amplitude,wide,deep,face};
  }
  const peaks=[
    crown(-66,-81,20,32,20,-.18),crown(-43,-72,23,28,20,-.13),
    crown(-27.5,-54.5,27.5,28,22,.20),crown(-15.8,-61,36,27,19,.10),
    crown(-5.4,-64,22.5,15,15,-.31),crown(4,-78,29,26,25,.11),
    crown(20.8,-66,31,24,19,-.24),crown(35,-58,22,31,24,-.13),
    crown(45,-76,27,26,23,.24),crown(67,-64,18,31,28,-.09),
  ];
  function mountain(x,z) {
    let height=0;
    for (const peak of peaks) {
      const dx=x-peak.x, dz=z-peak.z;
      const u=(dx+peak.face*dz)/peak.wide, v=(dz-peak.face*dx)/peak.deep;
      const mass=Math.max(0,1-Math.pow(Math.abs(u),1.14)-Math.pow(Math.abs(v),1.07));
      if (!mass) continue;
      const rib=Math.sin(dx*.49+dz*.31+peak.x)*.044+Math.sin(dx*.91-dz*.22)*.018;
      const shoulder=mass*(.82+mass*.22+rib)+Math.sin(dx*.18-dz*.21)*.054*mass;
      height=Math.max(height,peak.amplitude*shoulder);
    }
    // The low apron is broad underneath the flanks and pinches in under the cirque.
    const xWave=Math.sin(x*.118+.37);
    const shore=-31+3.5*xWave-5.6*Math.exp(-Math.pow((x+2)/10,2));
    const skirt=Math.min(6,Math.max(0,(shore-z)*(.20+.09*xWave)));
    return Math.max(height,skirt)-.12;
  }

  const tile=2.16;
  const columns=[];
  for (let z=-96;z<=-24;z+=tile) {
    for (let x=-85; x<=85;x+=tile) {
      const height=mountain(x,z);
      if (height<.25) continue;
      const crest=mountain(x,z+tile);
      const flank=Math.min(mountain(x-tile,z),mountain(x+tile,z));
      const slope=height-Math.min(crest,flank);
      const flute=Math.sin(x*.44+z*.18)+.46*Math.sin(x*.87-z*.37);
      const snowLine=7.1+3.2*Math.sin(x*.21-z*.091);
      const top=height<5.5?pick(bank):pick(height>snowLine?icy:bare);
      const deep=z<-73;
      const rockFace=deep?pick(['#64758d','#6a7e95','#73839b']):pick(bare);
      const y=Math.round(height/.27)*.27;
      const snowCap=height>snowLine && (slope<2.0+flute*.42||height>23.5);
      const tongue=height>4.3&&flute>1.03&&slope<2.1;
      if(snowCap||tongue) {
        const cap=Math.min(y-.04,.16+random()*.27+Math.max(slope,0)*.17);
        B(x,(y-cap-.35)*.5,z,tile,y-cap+.35,tile,rockFace);
        B(x,y-cap*.5,z,tile-.016,cap,tile-.014,deep?pick(distanceSnow):pick(snow));
      } else {
        B(x,(y-.35)*.5,z,tile,y+.35,tile,top);
      }
      if (y<6.2&&z>-39) columns.push([x,y,z]);
    }
  }

  function spruce(x,y,z,h) {
    const sp=h*(.37+random()*.09), offset=(random()-.5)*.18;
    B(x,y+h*.355,z,h*.078,h*.715,h*.081,'#536473');
    for (let level=0;level<5;level++) {
      const w=sp*(1-level*.153), y1=y+h*(.34+level*.139), lean=offset*level*.21;
      B(x+lean,y1,z,w,h*.18,w*.94,pick(pine));
      B(x+lean+w*.27,y1-.054*h,z+.03*h,w*.65,h*.109,w*.57,pick(pine));
      if (level<4) B(x+lean-w*.28,y1-.049*h,z-.04*h,w*.56,h*.108,w*.59,pick(pine));
      if (level>0 || random()>.49) B(x+lean+w*.14,y1+.079*h,z+.044*h,w*.72,h*.027,w*.77,pick(palePine));
    }
    B(x+offset,y+h*1.012,z,h*.084,h*.19,h*.084,pick(pine));
  }

  for (const [x,y,z] of columns) {
    const stripe=.6+.38*Math.sin(x*.24+z*.83);
    if(random()<stripe*.45) spruce(x+(random()-.5)*.70,y,z+(random()-.5)*.6,.77+random()*1.85);
  }

  // Snowy banks wrap around the open water. Snow color is supplied by the terrain boxes.
  function shore(side,z) {
    if(side<0) return -13.1+.59*Math.sin(z*.46)+2.4*Math.sin(z*.19)+Math.max(z+2,0)*.17;
    return 13.7+1.05*Math.sin(z*.28+2)+1.7*Math.sin(z*.45)+Math.max(z+7,0)*.22;
  }
  const shoreTile=1.36;
  const banks=[];
  for(const side of [-1,1]) {
    for(let z=-31.7;z<26;z+=shoreTile) {
      for(let j=0;j<=13;j++) {
        const x=side*(8+j*shoreTile), edge=shore(side,z);
        const inland=side*(x-edge);
        if(inland<-.17) continue;
        const y=.15+Math.min(3.6,inland*(.15+.07*Math.sin(z*.27))+Math.max(z-8,0)*.057)+random()*.098;
        const exposed=inland<1.28&&random()<.59;
        B(x,(y-.4)*.5,z,shoreTile,y+.4,shoreTile,exposed?pick(bare):pick(bank));
        if (!exposed) B(x,y+.053,z,shoreTile-.014,.106,shoreTile-.016,pick(snow));
        if(j<12) banks.push({side,x,y:y+(exposed?0:.106),z,inland});
      }
    }
  }
  function byShore(x,z) {
    return banks.find(p=>Math.abs(p.x-x)<=shoreTile*.501&&Math.abs(p.z-z)<=shoreTile*.501);
  }
  for(const {side,x,y,z,inland} of banks) {
    const clearing=side>0&&z>-17.3&&z<-6.1&&x<19.1&&x>12.1;
    if(clearing) continue;
    const cluster=Math.max(0,Math.sin(x*.62+z*.22)+.5*Math.sin(x*.84-z*.53));
    if(inland>2.3&&random()<(z>-3?.087:.17)+cluster*.066) {
      spruce(x+(random()-.5)*.39,y,z+(random()-.5)*.52,1.42+random()*2.1+(z>4?.7:0));
    } else if(random()<.22) {
      const step=.18+random()*.27;
      B(x+(random()-.5)*.61,y+step*.18,z,.27+random()*.50,step,.31+random()*.37,pick(bank),random()*.78);
    }
  }

  for(const [x,z,h] of [[-9.7,11.25,5.8],[-11.3,8.8,4.3],[-10.4,5.3,3.05],[15.3,10.7,4.8],[16.35,7.4,5.0]]) {
    const ground=byShore(x,z);
    if(ground) spruce(x,ground.y-.09,z,h);
  }

  // Small snowy skerries and thin shelves mark the shoreline on both sides of the lake.
  for(const [x,z,s] of [[-11.2,-16.44,1.15],[-9.7,-19.7,.66],[-9.1,6.3,.82],[11.0,-8.78,.41],[14.0,13.06,1.1],[10.14,-21.56,.73]]) {
    B(x,.066*s,z,1.31*s,.27*s,1.45*s,pick(bare),.26);
    B(x-.11*s,.190*s,z+.23*s,1.04*s,.228*s,.91*s,pick(bank),.24);
    B(x+.13*s,.296*s,z-.10*s,.68*s,.092*s,.72*s,pick(snow),.24);
  }
  for(let i=0;i<45;i++) {
    const z=-26+random()*52, side=random()<.53?-1:1;
    const x=shore(side,z)-side*(.32+random()*1.33);
    B(x,.001+random()*.010,z,.22+random()*1.05,.033,1.25*random()+.18,pick(['#9aafc0','#819bad','#8aa4b6']),random()*.31);
  }

  // Refuge, its split-log siding, slightly inset glowing windows and snow-laden roof.
  const homeX=16.20, homeZ=-11.64, base=1.01, angle=-.18;
  const sin=Math.sin(angle), cos=Math.cos(angle);
  function lodge(x,y,z,w,h,d,color) {
    B(homeX+x*cos+z*sin,base+y,homeZ-x*sin+z*cos,w,h,d,color,angle);
  }
  const logs=['#5a4b49','#614e49','#695751','#715b50','#65534e'];
  lodge(0,-.50,0,4.69,1.02,4.18,'#84949f');
  lodge(0,.075,0,4.17,.186,3.66,'#777a7d');
  lodge(0,.948,0,3.70,1.586,3.20,'#453f43');
  for (let course=0;course<8;course++) {
    const y=1.694+course*.177;
    lodge(0,y,0,Math.max(.14,(3.00-y)*2/.72-.13),.179,3.18,'#453f43');
  }
  for(let row=0;row<16;row++) {
    const y=.208+row*.177;
    const width=row<9?3.91:Math.max(.19,(3.00-y)*2/.72);
    lodge(0,y,-1.615,width,.165,.175,pick(logs));
    lodge(0,y,1.62,width,.166,.178,pick(logs));
    if(row<9) {
      lodge(-1.886,y,.00175,.191,.164,3.0585,pick(logs));
      lodge(1.883,y+.001,.00175,.192,.164,3.0585,pick(logs));
    }
  }
  for(let i=0;i<6;i++){
    const x=.144+i*.28;
    const y=3.015-x*.72;
    for(const side of [-1,1]){
      lodge(side*x,y,0,.292,.185,3.99,i%2?'#475565':'#4c5b69');
      lodge(side*x,y+.196,0,.286,.207,4.021,pick(snow));
      lodge(side*x,y-.082,1.988,.293,.144,.134,'#575860');
    }
  }
  for(let i=6;i<8;i++){
    const x=.144+i*.28,y=3.015-x*.72;
    for(const side of [-1,1]){
      lodge(side*x,y,0,.292,.186,4.02,'#4e5c6c');
      lodge(side*x,y+.192,0,.286,.203,4.045,pick(icy));
      lodge(side*x,y-.03,2.021,.291,.18,.11,'#595b65');
    }
  }
  lodge(0,3.135,0,.284,.188,4.15,'#aabed0');
  lodge(.82,2.97,-.88,.39,1.23,.41,'#666a73');
  lodge(.82,3.599,-.88,.46,.106,.47,'#4d5967');
  lodge(-.11,.753,1.754,.705,1.288,.109,'#473f43');
  lodge(-.15,.75,1.815,.579,1.147,.041,'#695447');
  lodge(.045,.715,1.843,.047,.042,.044,'#9b8464');
  for (const u of [-1.205,1.203]) {
    lodge(u,1.02,1.741,.657,.68,.118,'#3c3c45');
    lodge(u,1.023,1.820,.531,.557,.035,'#ffb455');
    lodge(u,1.021,1.846,.044,.559,.022,'#755548');
    lodge(u,1.137,1.847,.530,.041,.025,'#745748');
    lodge(u,.698,1.854,.720,.064,.175,'#8e8b87');
  }
  lodge(1.985,1.05,-.39,.104,.68,.76,'#44414a');
  lodge(2.045,1.055,-.39,.040,.55,.61,'#ffb455');
  lodge(2.071,1.055,-.39,.029,.55,.042,'#725449');
  lodge(2.072,1.133,-.39,.032,.046,.61,'#725549');
  lodge(-.15,.17,2.08,1.31,.137,.54,'#747576');
  lodge(-.15,.048,2.41,1.32,.12,.48,'#899aa9');
  lodge(.51,1.50,1.836,.095,.18,.088,'#ffb455');

  // Small leaning waymarkers show the short, sheltered approach above the water.
  for(let i=0;i<5;i++){
    const x=13.03+i*.5,z=-8.8-i*.31;
    const ground=byShore(x,z);
    if(ground) {
      const y=ground.y;
      B(x,y+.244,z,.063,.49,.065,'#6e645d');
      B(x,y+.464,z,.078,.070,.073,'#c0a07b');
    }
  }

  const snowOrigins=[[-9,-1],[-11,-12],[13,-3],[16,-17]].map(([x,z])=>{
    const v=banks.reduce((a,b)=>Math.hypot(b.x-x,b.z-z)<Math.hypot(a.x-x,a.z-z)?b:a);
    return [v.x,v.y,v.z];
  });
  return {snowOrigins,chimney:[homeX+.82*cos-.88*sin,base+3.66,homeZ-.82*sin-.88*cos]};
}
