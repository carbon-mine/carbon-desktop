/** Willow carr and a timber landing on a navigable fen channel. Water level is zero. */
export function buildMarsh(box) {
  let seed = 210981;
  const rand = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296);
  const range = (lo, hi) => lo + (hi - lo) * rand();
  const pick = values => values[Math.floor(rand() * values.length)];
  const tau = Math.PI * 2;
  const b = box;
  const peat = ['#51443e','#58473d','#5e4b40'];
  const sedge = ['#81834b','#838651','#797e48','#889154','#828a50'];
  const meadow = ['#64734f','#62744c','#6e8054','#728055'];
  const bark = ['#4e4842','#54493e','#5d5040','#665744'];
  const crown = ['#536e53','#607952','#648055','#6a8859','#738f5d','#648057'];
  const frond = ['#738b56','#839759','#91a15c','#9aa966','#819561','#748c58'];
  const wood = ['#87644b','#937456','#a07b59','#81614a','#95775d'];
  function solid(x,z,base,top,w,d,color,yaw=0) {
    b(x,(base+top)*.5,z,w,top-base,d,color,yaw);
  }
  const rowSize = 1.45;
  function rowCenter(z) { return Math.round(z/rowSize)*rowSize; }
  function edge(z, side) {
    z = rowCenter(z);
    const mid = 1.4+3.6*Math.sin(z*.102+.05)+1.95*Math.sin(z*.20-.35);
    const half = 6.65+.95*Math.sin(z*.15+.8);
    return Math.round((mid+side*half)*1.7)/1.7;
  }
  function shelf(z, side) {
    const r = rowCenter(z);
    return [1.48+.32*Math.sin(r*.29+side), 4.85+.85*Math.cos(r*.16-side), 10.9+.78*Math.sin(r*.14+side)];
  }
  function levels(z,side) {
    const r = rowCenter(z);
    return [.22+.075*Math.sin(r*.31+side), .50+.080*Math.cos(r*.19+side),
      .81+.12*Math.sin(r*.18+side),1.04+.17*Math.cos(r*.13+side)];
  }
  function ground(x,z) {
    const side = x < (edge(z,-1)+edge(z,1))*.5 ? -1 : 1;
    const d=(x-edge(z,side))*side;
    if (d<0) return -.26;
    const widths=shelf(z,side);
    const heights=levels(z,side);
    return heights[d<widths[0]?0:d<widths[1]?1:d<widths[2]?2:3];
  }
  for(let j=-51;j<=27;j++){
    const z=j*rowSize;
    for(const side of [-1,1]){
      const e=edge(z,side);
      const breaks=[0,...shelf(z,side), Math.abs(side*76-e)];
      const heights=levels(z,side);
      for(let k=0;k<4;k++){
        const x=e+side*(breaks[k+1]+breaks[k])*.5;
        const w=breaks[k+1]-breaks[k], top=heights[k], cap=k===0?.10:.15;
        solid(x,z,-1.05,top-cap,w,rowSize,pick(peat));
        const cover=k<2?sedge:meadow;
        const patch=.50+.25*Math.sin(x*.16+z*.073)+.23*Math.sin(z*.12-x*.065);
        solid(x,z,top-cap,top,w,rowSize,cover[Math.min(cover.length-1,Math.floor(patch*cover.length))]);
      }
    }
  }
  // A final reed bed closes the long sightline before the end of the water plane.
  solid(0,-76.175,-.9,.4,150,3.0,'#586a58');

  function branch(x1,y1,z1,x2,y2,z2,r,color) {
    const steps=Math.max(1,Math.ceil(Math.max(Math.abs(x2-x1),Math.abs(z2-z1),Math.abs(y2-y1))/(r*1.03)));
    for(let i=0;i<steps;i++){
      const t=(i+.5)/steps;
      b(x1+(x2-x1)*t,y1+(y2-y1)*t,z1+(z2-z1)*t,
        Math.abs(x2-x1)/steps+r,Math.abs(y2-y1)/steps+r*.80,
        Math.abs(z2-z1)/steps+r*.91,color);
    }
  }
  function willow(x,z,h,heading,detail=true) {
    const y=ground(x,z)-.04, R=h*.60;
    const ax=x+h*.078*Math.cos(heading), az=z+h*.078*Math.sin(heading);
    const fork=y+h*.39;
    // The flared base, asymmetric fork and exposed structural limbs carry the canopy.
    solid(x,z,y,y+h*.16,h*.145,h*.132,pick(bark),heading);
    branch(x,y+h*.12,z,ax,fork,az,h*.099,pick(bark));
    for(let i=0;i<5;i++){
      const a=heading+i*tau/5, d=h*range(.095,.16);
      branch(x+Math.cos(a)*d,y+.08,z+Math.sin(a)*d,x,y+h*(.095+i*.0025),z,h*.06,pick(bark));
    }
    const lobes=detail?7:6;
    for(let i=0;i<lobes;i++){
      const a=heading+i*tau/lobes+range(-.17,.17), r=R*range(.46,.75);
      const cx=ax+Math.cos(a)*r, cz=az+Math.sin(a)*r;
      const cy=y+h*range(.65,.81);
      const sx=ax+Math.cos(a)*r*.61, sz=az+Math.sin(a)*r*.61;
      branch(ax,fork,az,sx,cy-h*.07,sz,h*.06,pick(bark));
      branch(sx,cy-h*.07,sz,cx,cy,cz,h*.044,pick(bark));
      const w=h*range(.26,.39), d=h*range(.26,.36);
      b(cx,cy+h*.013,cz,w,h*.128,d,pick(crown),a*.21);
      b(cx-w*.067,cy+h*.092,cz+d*.06,w*.82,h*.093,d*.79,pick(crown),a*.19);
      b(cx-w*.090,cy+h*.152,cz+d*.048,w*.54,h*.060,d*.50,pick(crown),a*.17);
      // Smaller outward sprays interrupt the outline between the high crown and the pendants.
      for(let k=0;k<(detail?6:3);k++){
        const u=a+range(-1.75,1.75), dx=Math.cos(u), dz=Math.sin(u);
        const xx=cx+dx*w*.39, zz=cz+dz*d*.39, yy=cy+h*range(.011,.071);
        const sw=w*range(.23,.39),sd=d*range(.24,.37);
        b(xx,yy,zz,sw,h*.098,sd,pick(['#77905e','#6e8755','#83945d']),u*.20);
        b(xx+dx*sw*.22,yy-h*.061,zz+dz*sd*.20,sw*.67,h*.079,sd*.63,pick(frond),u*.25);
      }
      const count = detail?14:8;
      for(let j=0;j<count;j++){
        const u=j/count*tau+range(-.11,.11);
        const dx=Math.cos(u), dz=Math.sin(u), radial=range(.33,.46);
        const localX=dx*w*radial, localZ=dz*d*radial, angle=a*.21;
        let px=cx+Math.cos(angle)*localX+Math.sin(angle)*localZ;
        let pz=cz-Math.sin(angle)*localX+Math.cos(angle)*localZ;
        let py=cy+h*range(.005,.054);
        const length=h*range(.18,.40)*(0.84+.16*Math.cos(u-a));
        const breadth=h*range(.038,.070), phase=range(0,tau);
        const pigment=pick(frond), motion={axis:'z',amplitude:range(.017,.038),phase,pivot:[px,py,pz]};
        for(let n=0;n<3;n++){
          const height=length*[.31,.37,.32][n];
          const drift=[.34,.16,-.07][n]*breadth;
          px+=dx*drift; pz+=dz*drift;
          b(px,py-height*.5,pz,breadth*(1-n*.20),height,breadth*(.77-n*.13),pigment,a*.24,motion);
          py-=height;
        }
      }
    }
    b(ax,y+h*.84,az,h*.37,h*.15,h*.32,pick(crown),heading*.33);
    b(ax-h*.008,y+h*.936,az+h*.014,h*.23,h*.062,h*.24,pick(crown),heading*.29);
  }
  willow(-10.9,-6.6,9.55,-.65);
  willow(10.7,-14.3,8.3,2.54);
  willow(-10.4,-25.6,6.8,1.6,false);
  willow(13.9,-32.3,7.5,.65,false);
  willow(-19.2,3.3,8.4,2.2,false);
  willow(21.0,-4.5,8.3,.9,false);

  // Low alder silhouettes beyond the channel, tall and sparse around the open sky.
  function alder(x,z,h,distant) {
    const y=ground(x,z)-.08;
    solid(x,z,y,y+h*.80,h*.096,h*.10,distant?'#525863':'#514b45');
    const colors=distant?['#617078','#68777a','#697779']:['#526a61','#526d60','#5f775f'];
    for(let j=0;j<4;j++){
      const a=j*2.40, r=j===3?0:h*.115;
      const px=x+Math.cos(a)*r, pz=z+Math.sin(a)*r;
      b(px,y+h*(.59+j*.105),pz,h*(.40-j*.045),h*.255,h*(.385-j*.034),pick(colors),a*.21);
    }
  }
  for(let j=0;j<110;j++){
    const z=range(-72,-44), x=range(-62,60);
    if(ground(x,z)>0)alder(x,z,range(2.3,6.7)*(z<-62?1.14:1),true);
  }
  for(const side of [-1,1]){
    for(let j=0;j<14;j++){
      const z=range(-42,1), x=edge(z,side)+side*range(13,29);
      alder(x,z,range(3,6.3),false);
    }
  }

  // Every horizontal run has a disjoint footprint and timber bearers under its deck.
  const deck=.98, thickness=.15, half=.82;
  function span(axis,lo,hi,across) {
    const alongX=axis==='x';
    function place(s,t,y,w,h,d,color,yaw=0) {
      if(alongX)b(s,y,across+t,w,h,d,color,yaw);
      else b(across+t,y,s,d,h,w,color,yaw);
    }
    const n=Math.ceil((hi-lo)/.25), pitch=(hi-lo)/n, length=hi-lo;
    for(const t of [-.55,.55])place((lo+hi)/2,t,deck-thickness-.11,length,.22,.16,'#5d4a39');
    for(let j=0;j<n;j++){
      const s=lo+(j+.5)*pitch, shift=range(-.015,.015);
      place(s,shift,deck-thickness/2,pitch-.015,thickness,2*half+range(-.024,.024),pick(wood));
      if(j%3===0)for(const t of [-.54,.54])place(s,t,deck+.006,.032,.012,.035,'#554b43');
    }
    const pairs=Math.max(1,Math.ceil(length/2.25));
    for(let j=0;j<=pairs;j++){
      const s=lo+.18+(length-.36)*j/pairs;
      for(const t of [-.55,.55])place(s,t,(deck-.14-.72)/2,.20,deck-.14+.72,.21,'#665240');
    }
  }
  span('x',8.18,16.3,7);
  span('z',1.82,6.18,9);
  span('x',3.38,9.82,1);
  span('z',-7.8,.18,4.2);
  for(let j=0;j<4;j++){
    const z=-7.8-(j+.5)*.31;
    solid(4.2,z,.03,deck-(j+1)*.16,1.60,.304,pick(wood));
  }
  for(let j=0;j<3;j++){
    const x=16.3+(j+.5)*.31;
    solid(x,7,.12,deck-(j+1)*.135,.304,1.60,pick(wood));
  }
  // The lamp is suspended from an iron pin under a hooded oak bracket.
  function lamp(x,z,height,dir) {
    const stem=.19, top=deck+height, tip=x+dir*.66;
    solid(x,z,-.48,top,stem,stem,'#66523f');
    solid(x,z,deck+.09,deck+.23,.235,.23,'#414448');
    b(x+dir*.29,top-.055,z,.86,.15,.17,'#795d44');
    b(x+dir*.145,top-.20,z,.24,.15,.14,'#745a41');
    solid(tip,z,top-.27,top-.13,.054,.054,'#41454a');
    solid(tip,z,top-.33,top-.27,.22,.21,'#3e4245');
    solid(tip,z,top-.41,top-.33,.39,.36,'#50493f');
    solid(tip,z,top-.69,top-.41,.265,.246,'#ffb455');
    solid(tip,z,top-.75,top-.69,.35,.33,'#484441');
    for(const dx of [-.151,.151]) for(const dz of [-.14,.14])
      solid(tip+dx,z+dz,top-.69,top-.41,.033,.033,'#514841');
  }
  lamp(8.43,6.99,3.08,-1);
  lamp(3.68,.95,2.71,-1);
  lamp(4.70,-6.65,2.30,1);

  // A broad, shallow punt. The floor and cheeks follow the narrowing bow and stern.
  const boat={x:6.2,z:2.78,yaw:.055};
  function boatBox(s,y,t,w,h,d,color,angle=0){
    const c=Math.cos(boat.yaw),v=Math.sin(boat.yaw);
    b(boat.x+s*c+t*v,y,boat.z-s*v+t*c,w,h,d,color,boat.yaw+angle);
  }
  const stations=[.35,.49,.59,.635,.645,.635,.60,.54,.43,.31];
  for(let j=0;j<stations.length;j++){
    const s=(j-4.5)*.42, radius=stations[j], lift=.12*Math.pow(Math.abs(s)/1.89,2);
    boatBox(s,.048+lift*.30,0,.42,.135,radius*2,'#463e38');
    for(const sign of [-1,1]){
      boatBox(s,.182+lift*.65,sign*(radius-.053),.42,.27,.106,pick(['#6b503d','#735741','#765943']));
      boatBox(s,.331+lift*.65,sign*(radius-.053),.414,.043,.137,'#ae8758');
    }
    boatBox(s,.135+lift*.30,0,.398,.039,2*radius-.22,'#8c6949');
  }
  for(const [s,rr] of [[-2.046,.35],[2.046,.31]]) boatBox(s,.249,0,.105,.277,rr*2-.16,'#75573f');
  for(const [s,r] of [[-1.24,.57],[.74,.59]])boatBox(s,.299,0,.29,.088,2*r-.17,'#a67b50');
  // A pole rests across both seats with a blade just inboard of the transom.
  boatBox(-.20,.369,.14,2.95,.054,.056,'#bc9b69',.08);
  boatBox(1.33,.374,.018,.40,.064,.129,'#b99160',.08);
  solid(8.10,1.63,deck,deck+.31,.16,.17,'#634d39');
  // The painter drapes continuously from the dock cleat to the gunwale.
  const ropeStart=[8.10,deck+.22,1.63], ropeEnd=[8.10,.411,2.61];
  for(let j=0;j<12;j++){
    const t=(j+.5)/12, sag=.20*Math.sin(t*Math.PI);
    b(ropeStart[0],ropeStart[1]*(1-t)+ropeEnd[1]*t-sag,ropeStart[2]*(1-t)+ropeEnd[2]*t,
      .040,.040+(ropeStart[1]-ropeEnd[1]+.63*Math.cos(t*Math.PI))/12,.105,'#b39c74');
  }

  const reeds=['#77864f','#899453','#97a059','#798a54','#88975c'];
  function reedClump(x,z,h,wet) {
    for(let i=0;i<5;i++){
      const px=x+range(-.27,.27),pz=z+range(-.28,.28), py=ground(px,pz)-.03;
      const tall=h*range(.72,1.19);
      const a=range(0,tau), dx=Math.cos(a),dz=Math.sin(a);
      const width=range(.042,.074), col=pick(reeds);
      const joint=py+tall*.58;
      solid(px,pz,py,joint,width,width*.82,col,a);
      const movement={pivot:[px,joint,pz],axis:'z',amplitude:.065,phase:x*.37+z*.46+i*.28};
      b(px+dx*width*.43,joint+tall*.21,pz+dz*width*.39,width*.76,tall*.42,width*.73,col,a,movement);
      if(i===0&&wet){
        b(px+dx*width*.43,py+tall+.082,pz+dz*width*.39,.088,.175,.079,'#765844',a,movement);
      }
    }
  }
  for(const side of [-1,1]){
    for(let j=0;j<85;j++){
      const z=range(-38,21), offset=range(-.70,2.8), x=edge(z,side)+side*offset;
      if(x>2.6&&x<10.5&&z>-.05&&z<8.3)continue;
      reedClump(x,z,range(.57,1.3),offset<.8);
    }
  }
  // Duckweed and flowering lilies float along quiet edges outside the boat channel.
  function lily(x,z,size,bloom,yaw) {
    const c=Math.cos(yaw),s=Math.sin(yaw);
    function petal(u,v,y,w,h,d,col,a=0){
      b(x+c*u+s*v,y,z-s*u+c*v,w,h,d,col,yaw+a);
    }
    const leaf=pick(['#607c58','#718b5c','#7d995e','#6b8355']);
    // A missing wedge parts the two lobes, leaving the open water visible.
    petal(-.14*size,0,.028,.55*size,.046,.78*size,leaf);
    petal(.225*size,-.12*size,.028,.18*size,.046,.54*size,leaf);
    if(!bloom)return;
    const pink=pick(['#e4a0ae','#e99dad','#dab1c5','#e2a0bb']);
    for(let k=0;k<5;k++){
      const a=k*tau/5;
      petal(Math.cos(a)*.083,Math.sin(a)*.083,.084+k*.002,.117,.082,.070,pink,-a);
    }
    petal(0,0,.111,.102,.112,.10,'#e8bf75');
  }
    // Sedges break up the peat flats, with sword-shaped leaves and scattered waterside iris flowers.
  const grasses=['#75864e','#85945b','#94a263','#a4a867','#7b8c5c'];
  function tussock(x,z,size,flower) {
    const y=ground(x,z);
    if(y<0)return;
    const rotation=range(0,tau);
    for(let j=0;j<5;j++){
      const a=rotation+j*tau/5;
      const dx=Math.cos(a), dz=Math.sin(a), tall=size*range(.69,1.19);
      const foot=.08*size;
      const xx=x+dx*foot,zz=z+dz*foot,yy=ground(xx,zz)-.028;
      if(yy<0)continue;
      const leaf=pick(grasses),w=size*range(.09,.15), t=size*.09;
      b(xx,yy+tall*.22,zz,w,tall*.44,t,leaf,-a);
      b(xx+dx*w*.61,yy+tall*.59,zz+dz*w*.61,w*1.04,tall*.30,t*.85,leaf,-a);
      b(xx+dx*w*1.13,yy+tall*.87,zz+dz*w*1.13,w*.65,tall*.26,t*.70,leaf,-a);
    }
    if(flower){
      const h=size*1.42; solid(x,z,y-.02,y+h,.043,.041,'#687b51');
      const col=pick(['#b287bb','#aa83b5','#a98cb8','#cc9cb2']);
      for(let j=0;j<3;j++){
        const a=rotation+j*tau/3;
        b(x+Math.cos(a)*.074,y+h-.030+j*.003,z+Math.sin(a)*.074,.14,.065,.087,col,-a);
      }
      solid(x,z,y+h-.049,y+h+.059,.089,.087,'#d8b56b');
      b(x+.018,y+h+.075,z-.004,.080,.055,.071,'#ba9ac6');
    }
  }
  for(const side of [-1,1]){
    for(let i=0;i<74;i++){
      const z=range(-27,18),x=edge(z,side)+side*range(.20,12);
      if(x>2.9&&x<16.35&&z>-8&&z<8.2)continue;
      const size=range(.22,.58);
      tussock(x,z,size,i%10===0);
      if(i%4===0)tussock(x+range(-.38,.38),z+range(-.30,.30),size*.74,i%12===0);
    }
  }
  const placedPads=[];
  for(const [x,z,count] of [[-.8,8.3,17],[-5.8,1.2,13],[-7,-6.5,11],[2.55,-14,13],[11.3,8.7,13],[-3.35,14.1,18]]){
    for(let j=0;j<count;j++){
      const px=x+range(-1.6,1.6),pz=z+range(-1.45,1.45);
      if(ground(px,pz)>=0 || (px>3.05&&px<10&&pz>.05&&pz<8))continue;
      const size=range(.36,.64), radius=size*.55;
      if(placedPads.some(p=>Math.hypot(px-p.x,pz-p.z)<radius+p.radius+.035))continue;
      placedPads.push({x:px,z:pz,radius});
      lily(px,pz,size,j%4===0,range(0,tau));
    }
  }
  return {
    chimney:null,
    glowOrigins:[[-2.30,.70,7.6],[-8.2,.83,-3.6],[3.2,.76,-12.4],[11.55,.90,6.72]],
    lanternOrigins:[[7.77,3.51,6.99],[3.02,3.14,.95],[5.36,2.73,-6.65]],
  };
}




