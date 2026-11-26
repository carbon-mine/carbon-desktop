/** An early-morning mixed farm. X and Z are in metres, and the yard is at Y=0. */
export function buildFarm(box) {
  let state = 94073;
  const rand = () => ((state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296);
  const choose = colors => colors[Math.floor(rand() * colors.length)];
  const turf = ['#607840','#6e864b','#769452','#80914b','#738a49','#6a834a'];
  const leaves = ['#537439','#668e40','#739a48','#83ab52','#4e733c','#67994a'];
  const darkLeaves = ['#476734','#416132','#547637','#5b7939','#3e602c'];
  const golden = ['#c48739','#d29a41','#deab51','#e5bb5e','#cf9943','#d4a449'];
  const cropSoil = ['#533b28','#5c412b','#60432d','#5a3e2b'];
  const fenceWood = ['#87644b','#937456','#a07b59','#81614a','#95775d'];
  const barnBoards = ['#8e4935','#974d39','#a4563e','#9e5038','#ab5940','#884633'];
  const whitewash = '#c9aa81';
  const dark = '#342d28';
  const B = box;

  function model(x, z, yaw, scale = 1, movement) {
    const cos = Math.cos(yaw), sin = Math.sin(yaw);
    return (u,y,v,w,h,d,c,turn=0) =>
      B(x+(u*cos+v*sin)*scale,y*scale,z+(-u*sin+v*cos)*scale,
        w*scale,h*scale,d*scale,c,yaw+turn,movement);
  }

  function beam(x1,z1,x2,z2,y,w,h,c) {
    const dx = x2-x1, dz = z2-z1;
    B((x1+x2)/2,y,(z1+z2)/2,w,h,Math.hypot(dx,dz),c,Math.atan2(dx,dz));
  }

  function tuft(x,z,h,c,y=.004) {
    const b=model(x,z,rand()*Math.PI);
    b(0,y+h*.49,0,.043,h,.050,c);
    b(.046,y+h*.36,-.025,.052,h*.73,.057,c);
    b(-.041,y+h*.32,.018,.058,h*.65,.047,c);
    b(-.074,y+h*.21,.012,.054,h*.43,.056,c);
  }

  B(0,-.325,-14,170,.65,132,'#6c8546');
  // Seeded little rises and a wooded boundary break the horizon into rolling silhouettes.
  for (const [z,maxHeight,density,palette] of [
    [-62,5.7,2.05,['#708270','#738472','#788877']],
    [-47,3.7,1.62,['#617653','#687e59','#6b805d']],
    [-35,1.55,1.79,['#5c7547','#60794b','#657d4c']],
  ]) {
    for (let x=-72;x<73;x+=density) {
      const wave=.53+.22*Math.sin(x*.14+z)+.15*Math.cos(x*.12-z)+.08*Math.sin(x*.34);
      const h=maxHeight*wave, w=density+.018;
      const deepZ=z+1.9*Math.sin(x*.11+z);
      B(x,h*.5-.088,deepZ,w,h+.176,7.6,choose(palette));
      if (rand()<.88) {
        const crown=.32+rand()*1.06, treeX=x+(rand()-.5)*.7, treeZ=deepZ+.7*rand();
        B(treeX,h+crown*.54,treeZ,.38+crown*.58,crown*1.12,.58,choose(palette));
        B(treeX+.19,h+crown*.38,treeZ+.26,.58+crown*.49,crown*.67,.63,choose(palette));
        if (rand()<.69) B(treeX-.16,h+crown*1.12,treeZ,.26+crown*.34,crown*.61,.48,choose(palette));
      }
    }
  }

  function tree(x,z,s) {
    const b=model(x,z,rand()*3,s);
    const height=2.46+rand()*.62;
    const autumn=rand()<.29;
    const crown=autumn?['#60713e','#535f31','#807b43','#747944','#697947']:darkLeaves;
    b(0,height*.49,0,.23,height,.25,'#644833');
    b(-.092,height*.72,.034,.232,.69,.209,'#705238');
    b(.29,height*.76,.03,.156,.96,.147,'#68533b');
    b(.18,height*.79,.153,.51,.137,.19,'#725a3d');
    b(-.225,height*.89,-.023,.439,.164,.158,'#785a3b');
    const centers=[[-.67,-.12,.17],[.53,.12,.24],[-.26,.45,-.52],[.13,.26,.54],[.24,.70,.01]];
    for (const [u,y,v] of centers) {
      const size=.54+rand()*.51;
      const py=height+y-.22+(rand()-.5)*.36;
      b(u,py,v,size,.60+rand()*.52,size*.92,choose(crown));
      b(u+.12,py-.22,v+.19,size*.63,.44,size*.79,choose(crown));
      for (let j=0;j<4;j++) {
        const theta=rand()*Math.PI*2;
        const span=size*(.31+rand()*.21);
        const edge=.19+rand()*.37;
        b(u+Math.cos(theta)*span,py+(rand()-.38)*.77,v+Math.sin(theta)*span,
          edge,edge*.81,edge*.93,choose(rand()<.38?leaves:crown));
      }
    }
  }
  for (const [x,z,s] of [
    [-22,-13,1.62],[-19.7,-14.2,1.04],[-16,-18,1.38],[-12.1,-21,.89],[-9.6,-20,1.31],[-6,-21,.76],
    [2.65,-17,1.65],[5.2,-19.6,1.19],[9,-20,.91],[13.2,-22,1.18],[18,-20,1.58],[21,-17.4,1.07],
    [26,-9,1.76],[-24,0,1.68],[-21.7,1.1,.98],[-19.6,3.7,1.28],[23,2,1.25],[26,4,1.46],
  ]) tree(x,z,s);

  function hedge(x1,z1,x2,z2) {
    const count=Math.ceil(Math.hypot(x2-x1,z2-z1)/.70);
    for (let i=0;i<count;i++) {
      if(rand()<.13) continue;
      const t=(i+rand()*.7)/count;
      const x=x1+(x2-x1)*t+(rand()-.5)*.66, z=z1+(z2-z1)*t+(rand()-.5)*.72;
      const h=.28+rand()*.47, w=.40+rand()*.58;
      B(x,h*.45,z,w,h*.9,.48+rand()*.38,choose(darkLeaves),rand()*.36);
      B(x+.043,h*.86,z+.045,w*.67,h*.52,.50,choose(turf),rand()*.43);
      if(rand()<.43) B(x+.31,h*.59,z-.013,w*.43,h*.46,.48,choose(leaves));
    }
  }
  hedge(-27,-9,-19,-15);
  hedge(-19,-15,-8.75,-14);
  hedge(-22,-1,-20.5,6.75);
  hedge(1.2,-16,10.6,-15.5);
  hedge(12.8,-15.8,24.2,-12.1);
  hedge(16.7,-3.9,17.2,6.6);
  hedge(19.3,6.2,20.0,11.2);

  const tractorX=-5.45, tractorZ=4.7, heading=Math.PI/2, tractorSize=1.22;
  const lane = [
    [5.9,23],[4.2,16],[2.9,10],[2.1,5],[1.4,.7],[-.2,-3.2],
  ];
  for (let i=1;i<lane.length;i++) {
    const [x1,z1]=lane[i-1], [x2,z2]=lane[i];
    const y=.035+i*.001;
    beam(x1,z1,x2,z2,y,2.25,.034,'#a38e65');
    beam(x1-.62,z1,x2-.62,z2,y+.02,.36,.015,'#907752');
    beam(x1+.61,z1,x2+.61,z2,y+.024,.35,.014,'#927853');
  }
  // A spring seedbed between the market garden and the yard. Work along its long axis.
  const field=model(tractorX,tractorZ,heading);
  const nearEnd=-8.15, farEnd=5.15, halfWidth=1.53;
  field(0,.002,(nearEnd+farEnd)/2,halfWidth*2,.02,farEnd-nearEnd,'#624730');
  // Long ridges of damp earth with short, irregular breaks and last season's stubble.
  // A separate seed keeps small soil edits from reshuffling the surrounding landscape.
  let soilSeed=801;
  const soilRand=()=>((soilSeed=(Math.imul(soilSeed,1664525)+1013904223)>>>0)/4294967296);
  for(let row=0;row<14;row++){
    const u=(row-6.5)*.218;
    for(let v=nearEnd+.05;v<farEnd-.06;){
      const length=Math.min(.34+soilRand()*.86,farEnd-.04-v);
      const height=.017+soilRand()*.023;
      const worked=v < -2.9 && Math.abs(u)<.88;
      field(u,.012+height/2,v+length/2,.102+soilRand()*.045,height,length-.012,
        (worked?['#563c29','#63432e','#60412a']:['#765438','#805c3e','#705036'])[Math.floor(soilRand()*3)]);
      if(!worked && soilRand()<.22)
        field(u+.035,.023+height,v+length*.53,.025,.044,.052,'#aa8350',.17);
      v+=length;
    }
  }
  function onTurnout(x,z) {
    const dx=x-tractorX,dz=z-tractorZ;
    const along=dx*Math.sin(heading)+dz*Math.cos(heading);
    const across=dx*Math.cos(heading)-dz*Math.sin(heading);
    return along>=nearEnd-.10 && along<=farEnd+.10 && Math.abs(across)<halfWidth+.1;
  }
  B(-.61,.058,-4.30,9.8,.072,4.05,'#a08a60');
  for (let i=0;i<125;i++) {
    const z=rand()*29-4.1;
    const x=Math.max(-.4,(z+4.5)*.221)+(rand()-.5)*1.72;
    const h=.018+rand()*.028;
    if(onTurnout(x,z)) continue;
    B(x,.073+h*.5,z,.07+rand()*.17,h,.05+rand()*.18,choose(['#ad9670','#b49c74','#887654','#9d8b68']),rand()*2.5);
  }

  // Market greens in staggered rows, with waxy leaf tips catching the sun.
  for (let row=0;row<9;row++) {
    const z=6.58+row*.87, xStart=-13.60, xEnd=-.31;
    B((xStart+xEnd)/2,.048,z,xEnd-xStart,.074,.73,choose(cropSoil));
    B((xStart+xEnd)/2,.094,z,xEnd-xStart-.15,.023,.50,choose(['#715038','#77553a','#6d4931']));
    for (let x=xStart+.25;x<xEnd-.12;x+=.47) {
      if (rand()<.046) continue;
      const p=x+(rand()-.5)*.19, q=z+(rand()-.5)*.13;
      const h=.14+rand()*.16, spin=rand()*6.28, c=choose(leaves);
      const b=model(p,q,spin);
      b(0,.14+h*.45,0,.128,h,.116,choose(darkLeaves));
      b(0,.14+h*.34,0,.42,h*.43,.124,c);
      b(0,.16+h*.60,0,.14,h*.48,.36,c);
      b(.075,.18+h*.64,.025,.20,h*.55,.183,choose(['#80a84d','#849f45','#8eaf57','#608945']));
      if (rand()<.14) b(.06,.19+h*.96,-.02,.065,.06,.07,'#adc97a');
    }
  }

  for (let row=0;row<15;row++) {
    const x=-18+row*.69, zStart=-11.5+.05*row, zEnd=3.1-.15*row;
    B(x,.051,(zStart+zEnd)/2,.56,.071,zEnd-zStart,choose(cropSoil));
    for (let z=zStart+.18;z<zEnd;z+=.31) {
      const p=x+(rand()-.5)*.29, q=z+(rand()-.5)*.24;
      const h=.33+rand()*.39;
      B(p,.087+h*.5,q,.038,h,.035,choose(['#b4994c','#a88437','#c29c4c']));
      B(p+.009,.088+h,q,.104,.181,.088,choose(golden),rand()*4);
      if (rand()<.66) B(p+.038,.16+h,q+.002,.075,.114,.068,choose(golden));
    }
  }

  const meadowGrassTints = ['#78914b','#72934e','#819f55','#93a55f','#698247','#78994f'];
  for (let i=0;i<385;i++) {
    const x=rand()*52-26, z=rand()*43-13;
    const pathX=Math.max(-.4,(z+4.5)*.221);
    if ((x>-13.98&&x<.12&&z>6.13&&z<14.04) ||
        (x>-18.5&&x<-7.9&&z>-11.8&&z<3.2) ||
        (z>-4.8&&Math.abs(x-pathX)<1.22) || onTurnout(x,z) ||
        (z<-3.2&&z>-13.3&&x>-8.4&&x<3.7)) continue;
    const h=.12+rand()*.28;
    if (z>5 && rand()<.47) tuft(x,z,h,choose(meadowGrassTints));
    else B(x,.02+h*.5,z,.07+rand()*.065,h,.073,choose(leaves),rand()*3);
    if (rand()<.136) B(x,.046+h,z+.01,.107,.057,.091,choose(['#eee1a9','#dbc265','#bb91a4','#e4ddc9']));
  }

  const flowerColors = ['#e9dec1','#f1e6cb','#dfbd65','#e8c66f','#ca9aa4','#d6aaa9'];
  function flowers(x,z,h,color) {
    const b=model(x,z,rand()*3.14);
    b(0,h*.5,0,.029,h,.033,'#587b41');
    b(.071,h*.39,-.048,.024,h*.78,.023,'#719051');
    b(-.062,h*.31,-.054,.024,h*.62,.024,'#688945');
    b(0,h,.002,.098,.052,.084,color);
    b(.069,h*.79,-.046,.073,.043,.067,color);
    b(-.06,h*.64,-.054,.067,.044,.069,color);
  }
  function meadowPatch(x,z,width,depth,count,chance) {
    for (let i=0;i<count;i++) {
      const u=(rand()-.5)*width, v=(rand()-.5)*depth;
      if (Math.abs(u)/width+Math.abs(v)/depth>.73) continue;
      const p=x+u, q=z+v;
      if(onTurnout(p,q) || (p>-13.98&&p<.12&&q>6.13&&q<14.04)) continue;
      const h=.091+rand()*.27;
      tuft(p,q,h,choose(meadowGrassTints));
      if (rand()<chance) flowers(p+.095,q-.067,h+.10,choose(flowerColors));
    }
  }
  for (const patch of [
    [-12.0,7.3,3.2,4.1,23,.29],[-11.5,12.5,3.4,4.2,21,.32],
    [-8.2,16.4,5.4,3.3,26,.28],[-3.4,15.6,5.6,3.5,42,.26],
    [-2.1,11.0,2.5,6.2,30,.26],[.21,7.3,1.43,3.8,16,.40],
    [.7,11.8,1.30,3.7,19,.42],[1.75,18.8,2.5,6.7,30,.42],
    [5.4,8.8,2.1,1.3,26,.45],[8.5,8.55,3.4,1.5,32,.41],
    [8.91,11.3,5.25,3.48,50,.30],[7.5,14.9,4.2,3.2,43,.21],
    [12.1,15.35,4.7,5.4,38,.35],[15.45,9.4,2.2,2.8,34,.42],
    [16.1,14.2,4.5,5.6,45,.29],[20.8,17.2,4.2,5.1,26,.38],
    [5.22,3.4,1.1,4.7,10,.24],[15.12,5.68,1.1,2.9,13,.30],
    [-16.8,2.53,3.5,1.51,14,.31],[-19.0,-.91,1.7,3.8,20,.30],
  ]) meadowPatch(...patch);
  const fencePosts=new Set();
  function fence(x1,z1,x2,z2,gap=-1) {
    const count=Math.ceil(Math.hypot(x2-x1,z2-z1)/1.7);
    for (let i=0;i<=count;i++) {
      const x=x1+(x2-x1)*i/count, z=z1+(z2-z1)*i/count;
      const postColor=choose(fenceWood), key=x.toFixed(2)+','+z.toFixed(2);
      if(!fencePosts.has(key)){
        fencePosts.add(key);
        B(x,.52,z,.153,1.04,.147,postColor);
        B(x,1.043,z,.162,.083,.155,'#ab8864');
      }
      if (i===count||i===gap) continue;
      const nextX=x1+(x2-x1)*(i+1)/count, nextZ=z1+(z2-z1)*(i+1)/count;
      beam(x,z,nextX,nextZ,.70,.058,.115,choose(fenceWood));
      beam(x,z,nextX,nextZ,.39,.064,.101,choose(fenceWood));
    }
  }
  fence(4.6,-5.65,15.7,-5.65);
  fence(15.7,-5.65,15.7,7.65);
  fence(15.7,7.65,4.6,7.65,5);
  fence(4.6,7.65,4.6,-5.65);
  fence(-18.5,-12,-7.63,-12);

  // Weatherboard barn, with a stepped standing-seam roof and light trim.
  const barn=model(-3.4,-8.7,0);
  barn(0,.147,0,7.32,.294,5.70,'#736a55');
  for (let i=0;i<37;i++) {
    const u=-3.48+i*.193;
    const height=3.15+(1-Math.abs(u)/3.65)*1.79;
    barn(u,.29+height*.5,2.70,.186,height,.17,choose(barnBoards));
    barn(u,.29+height*.5,-2.70,.186,height,.17,choose(barnBoards));
  }
  for (let i=0;i<28;i++) {
    const v=-2.60+i*.194;
    const east=v>-.95&&v<.55;
    const c=choose(barnBoards);
    barn(3.52,east?.81:1.865,v,.235,east?1.04:3.15,.187,c);
    if (east) barn(3.52,2.982,v,.235,.916,.187,c);
    barn(-3.52,1.865,v,.234,3.15,.187,choose(barnBoards));
  }
  for (const side of [-1,1]) {
    for (let i=0;i<18;i++) {
      const u=(i+.32)*.226, y=5.43-u*.505;
      barn(u*side,y,0,.235,.175,6.16,choose(['#51534e','#535650','#575953','#58564e','#575a55']));
      for (let v=-2.88;v<3.0;v+=.79) barn(u*side,y+.091,v,.222,.015,.034,'#444945');
      barn(u*side,y-.07,3.075,.232,.16,.083,'#b29570');
    }
    barn(side*3.82,3.53,0,.115,.22,6.06,'#554b3c');
    barn(side*3.56,1.84,2.813,.139,3.14,.16,whitewash);
  }
  barn(0,5.49,0,.304,.164,6.27,'#555750');
  barn(0,2.85,2.814,7.03,.115,.139,'#a87b5b');
  barn(0,1.47,2.829,2.83,2.36,.207,'#3d3028');
  for (const side of [-1,1]) {
    const x=side*.697;
    for (let i=0;i<8;i++) barn(x-.56+i*.163,1.434,2.978,.155,2.28,.114,choose(barnBoards));
    for (const y of [.38,2.54]) barn(x,y,3.043,1.319,.117,.065,whitewash);
    for (const u of [x-.617,x+.617]) barn(u,1.46,3.056,.102,2.11,.075,whitewash);
    for (let i=0;i<9;i++) barn(x+(i-4)*.133,1.46+(i-4)*.224*side,3.056,.172,.231,.084,'#bfa582');
    barn(x-side*.115,1.5,3.106,.085,.087,.034,'#403833');
  }
  barn(0,2.69,3.03,3.02,.155,.23,whitewash);

  function frontWindow(u,y,w,h) {
    barn(u,y,2.811,w+.18,h+.18,.131,whitewash);
    barn(u,y,2.891,w,h,.055,'#443a30');
    for (const offset of [-.25,.25]) barn(u+offset*w,y+.045,2.928,w*.37,h*.74,.021,'#78644b');
    barn(u,y,2.953,.054,h,.026,whitewash);
    barn(u,y,2.957,w,.045,.03,whitewash);
    barn(u,y-h*.5-.099,2.917,w+.28,.093,.21,'#b49b77');
  }
  frontWindow(-2.53,1.95,.58,.67);
  frontWindow(2.53,1.95,.58,.67);
  frontWindow(0,3.83,.78,.68);
  barn(3.49,1.84,-.2,.09,1.1,1.59,whitewash);
  barn(3.56,1.84,-.2,.061,.85,1.34,'#504738');
  for (const v of [-.56,-.16,.24]) barn(3.61,1.84,v,.065,.75,.053,whitewash);
  barn(3.59,1.847,-.2,.047,.046,1.34,whitewash);
  barn(0,.323,3.14,3.12,.106,.53,'#a79575');

  // Grain silo uses inset chord strips so the corrugated round profile stays voxel-built.
  const silo=model(-9.05,-8.86,0);
  for (let ring=0;ring<17;ring++) {
    const y=.105+ring*.237;
    const r=ring<14?1.08:1.08-(ring-13)*.224;
    const bands=ring<14?11:7;
    for (let i=0;i<bands;i++) {
      const u=(2*i+1-bands)*r/bands;
      const d=2*Math.sqrt(r*r-u*u);
      silo(u,y,0,2*r/bands-.005,.232,d,
        ring<14?choose(['#9b9c8d','#a3a594','#989b8d','#afb19e']):choose(['#616e61','#6b7667','#637064']));
    }
  }
  silo(0,4.092,0,.315,.385,.30,'#646a5e');
  for (let i=0;i<11;i++) silo(-.75,.37+i*.298,.919,.263,.045,.062,'#59574b');
  for (const u of [-.897,-.607]) silo(u,1.88,.911,.054,3.37,.062,'#655f52');

  function wheel(b,x,y,z,r,w, tire='#343935') {
    const step=r*2/11;
    for (let j=0;j<11;j++) {
      const v=(5-j)*step;
      const depth=2*Math.sqrt(r*r-v*v);
      b(x,y+v,z,w,step-.003,depth,tire);
      if(j%2===0) {
        for (const sign of [-1,1]) b(x,y+v,z+sign*(depth*.5-.055),w+.075,step*.59,.10,'#4c5047');
      }
    }
    const rim=r*.53;
    for (let j=0;j<7;j++) {
      const v=(3-j)*rim*2/7;
      b(x,y+v,z,w+.030,rim*2/7-.003,2*Math.sqrt(rim*rim-v*v),'#b5b6a8');
    }
    b(x,y,z,w+.089,r*.33,r*.34,'#858d83');
  }

  // An orchard tractor with an offset exhaust, exposed engine, and oversized driven wheels.
  const drive={drive:true,heading};
  const tractor=model(tractorX,tractorZ,heading,tractorSize,drive);
  function drivenWheel(u,y,v,r,w){
    const ca=Math.cos(heading),sa=Math.sin(heading);
    const axle={...drive,radius:r*tractorSize,
      pivot:[tractorX+(u*ca+v*sa)*tractorSize,y*tractorSize,tractorZ+(-u*sa+v*ca)*tractorSize]};
    wheel(model(tractorX,tractorZ,heading,tractorSize,axle),u,y,v,r,w);
  }
  const paint='#6d9950', highlighted='#95ae6b', metal='#55584e';
  for (const sign of [-1,1]) {
    drivenWheel(sign*.828,.712,-.58,.712,.47);
    drivenWheel(sign*.665,.397,1.05,.397,.253);
    // The stepped tire's outer corners sweep beyond its nominal radius.
    // Set each fender underside from its nearest edge to the axle.
    const clearanceRadius=.813, panelDepth=.224, panelHeight=.085;
    for(let i=-3;i<=3;i++){
      const offset=i*panelDepth;
      const nearest=Math.max(0,Math.abs(offset)-panelDepth/2);
      const underside=.712+Math.sqrt(clearanceRadius**2-nearest**2);
      tractor(sign*.828,underside+panelHeight/2,-.58+offset,.60,panelHeight,panelDepth,paint);
      if(i!==0){
        const innerEdge=Math.max(0,nearest-panelDepth);
        const rise=Math.sqrt(clearanceRadius**2-innerEdge**2)-(underside-.712);
        tractor(sign*.828,underside+panelHeight+rise/2,-.58+offset-Math.sign(i)*(panelDepth-.038)/2,.60,rise,.038,paint);
      }
    }
    tractor(sign*.395,1.50,-.58,.306,.07,.12,metal);
    tractor(sign*.29,1.23,-.58,.08,.58,.12,metal);
    tractor(sign*.544,.476,-.045,.25,.074,.39,metal);
    tractor(sign*.428,.75,.53,.041,.23,.32,'#3d4741');
    for (let i=0;i<5;i++) tractor(sign*.449,.775,.408+i*.077,.025,.096,.032,'#b2ae82');
    tractor(sign*.27,.979,1.587,.142,.133,.045,'#ead4a1');
  }
  tractor(0,.416,1.05,1.43,.10,.095,metal);
  tractor(0,.688,-.58,1.56,.166,.157,metal);
  tractor(0,.626,.38,.66,.285,2.28,metal);
  tractor(0,1.035,.75,.858,.399,1.61,paint);
  tractor(0,1.269,.74,.762,.094,1.51,highlighted);
  tractor(0,.935,1.557,.61,.565,.13,'#565b4d');
  for (let u=-.25;u<.27;u+=.062) tractor(u,.935,1.633,.025,.33,.022,'#978f74');
  tractor(0,.717,-.64,.62,.118,.56,'#525349');
  tractor(0,1.003,-.53,.42,.107,.385,'#5d4b3e');
  tractor(0,1.12,-.711,.369,.216,.087,'#695548');
  tractor(0,1.29,-.143,.055,.34,.064,'#3c433d');
  tractor(0,1.431,-.146,.274,.072,.181,'#363e39');
  tractor(-.231,1.57,.86,.084,.677,.084,'#4a514a');
  tractor(-.232,1.902,.87,.118,.112,.147,'#565d53');
  tractor(.217,1.374,.47,.125,.203,.113,'#5a5c51');
  tractor(0,.574,1.772,.99,.13,.286,'#656c5b');
  for (const u of [-.455,.455]) {
    tractor(u,1.011,1.512,.115,.133,.156,'#6f7866');
    tractor(u,1.011,1.606,.093,.102,.041,'#eeddaf');
  }
  tractor(.432,1.166,.758,.021,.049,.68,'#d6c480');
  tractor(-.432,1.165,.755,.022,.049,.68,'#d6c480');
  // A braced drawbar and vertical pin join the harrow tongue at the rear.
  tractor(0,.45,-1.205,.205,.105,1.15,metal);
  tractor(0,.54,-1.735,.114,.262,.101,'#9a9682');
  tractor(0,.681,-1.735,.167,.041,.132,metal);

  // The driver rides with the body. Hands land at the rim of the steering wheel.
  const jacket='#54747b', denim='#475b64', skin='#be8e68', boots='#423a30';
  tractor(0,1.12,-.487,.30,.17,.31,denim);
  tractor(0,1.387,-.515,.337,.414,.23,jacket);
  tractor(0,1.638,-.49,.115,.102,.103,skin);
  tractor(0,1.763,-.472,.233,.236,.228,skin);
  tractor(0,1.901,-.501,.271,.104,.248,'#936044');
  tractor(0,1.875,-.348,.273,.040,.119,'#a97550');
  for (const sign of [-1,1]) {
    tractor(sign*.109,1.067,-.33,.119,.14,.292,denim);
    tractor(sign*.115,.925,-.189,.105,.197,.121,denim);
    tractor(sign*.112,.827,-.143,.132,.092,.181,boots);
    tractor(sign*.203,1.484,-.452,.114,.167,.17,jacket);
    tractor(sign*.171,1.436,-.333,.113,.102,.176,jacket);
    tractor(sign*.104,1.437,-.227,.096,.092,.111,skin);
  }

  // A compact loader waits beside the barn with its bucket lowered.
  const loader=model(3.67,-7.23,.79,.82);
  const ochre='#b56d3d', bright='#d89a51', iron='#64675d';
  for (const u of [-.635,.635]) {
    wheel(loader,u,.45,-.66,.45,.318);
    wheel(loader,u,.36,.79,.36,.271);
    loader(u,.904,-.66,.350,.11,.91,ochre);
    loader(u,.509,.18,.114,.119,1.40,ochre);
    for (let i=0;i<6;i++) loader(u,.57-i*.067,.69+i*.17,.108,.115,.23,i%2?ochre:bright);
    loader(u,.264,1.61,.14,.22,.10,iron);
    loader(u*.89,1.14,-.77,.082,1.28,.083,'#5a5848');
    loader(u*.88,1.47,.02,.081,.68,.081,'#645b47');
  }
  loader(0,.482,0,1.19,.17,1.81,iron);
  loader(0,.77,.58,.83,.37,1.12,ochre);
  loader(0,.987,.54,.804,.089,1.109,bright);
  loader(0,.778,1.151,.66,.278,.103,'#514c40');
  for (let u=-.26;u<=.27;u+=.071) loader(u,.786,1.212,.041,.205,.029,'#8b8573');
  loader(0,.736,-.56,.472,.162,.412,'#514238');
  loader(0,.899,-.683,.435,.29,.087,'#635143');
  loader(-.218,1.13,.46,.075,.46,.088,iron);
  loader(0,1.851,-.35,1.415,.128,1.062,'#bfa36c');
  loader(0,1.921,-.35,1.282,.043,.954,bright);
  loader(0,.108,1.876,1.70,.12,.77,iron);
  loader(0,.309,1.578,1.70,.323,.121,'#796e54');
  for (const u of [-.797,.796]) loader(u,.234,1.874,.104,.243,.687,'#676557');
  for (let i=0;i<6;i++) loader(-.65+i*.26,.089,2.299,.119,.071,.181,'#77796c');
  // Towed spring-tooth harrow, with a wood wagon and loose tools in the barn yard.
  const harrow=model(tractorX-2.92*Math.sin(heading),tractorZ-2.92*Math.cos(heading),heading,1.12,drive);
  for (const v of [-.59,0,.59]) {
    harrow(0,.408,v,1.62,.088,.095,'#8a5941');
    for (const u of [-.70,-.46,-.22,.08,.35,.62]) {
      harrow(u,.213,v,.043,.317,.047,'#5d5a4e');
      harrow(u,.043,v-.045,.064,.075,.149,'#535248');
    }
  }
  for (const u of [-.679,.677]) harrow(u,.42,.001,.083,.093,1.34,'#986b4b');
  harrow(0,.47,.654,.135,.092,.192,'#786d57');
  for (const sign of [-1,1]) {
    harrow(sign*.33,.432,.481,.078,.075,.82,'#986b4b',-sign*.86);
  }

  const wagon=model(-5.15,-4.42,.25);
  for (const u of [-.83,.83]) {
    wheel(wagon,u,.408,-.62,.408,.15,'#524b3c');
    wheel(wagon,u,.32,.58,.32,.14,'#524b3c');
    for (let i=0;i<4;i++) wagon(u,.69+i*.117,0,.067,.107,1.875,choose(fenceWood));
  }
  wagon(0,.60,0,1.62,.14,1.88,'#86654a');
  for (const v of [-.93,.93]) wagon(0,.885,v,1.65,.50,.072,'#a1835c');
  wagon(0,.49,1.53,.162,.101,1.34,'#80664b');
  for (const [u,v] of [[-.35,-.53],[.22,-.52],[-.21,.18],[.32,.4]]) {
    wagon(u,.856,v,.47,.43,.49,choose(golden));
    wagon(u,.862,v,.031,.437,.502,'#987341');
  }

  for (const [x,z,angle] of [[-.13,-4.02,.41],[.43,-4.16,-.43],[-6.48,-5.36,.21]]) {
    const tool=model(x,z,angle);
    tool(0,.103,0,.066,.061,1.0,'#a18860');
    tool(0,.086,.52,.28,.095,.21,'#64685c');
    tool(0,.114,-.468,.119,.075,.125,'#675b45');
  }
  for (let i=0;i<6;i++) {
    const x=-7.44+Math.floor(i/2)*.55, z=-5.22+(i%2)*.56;
    B(x,.216,z,.518,.386,.471,choose(golden),.16);
    B(x,.415,z,.099,.031,.48,'#977341',.16);
  }

  function animalHead(x,z,angle,scale,y,front,amount,phase) {
    const dominant=Math.abs(Math.cos(angle))>=Math.abs(Math.sin(angle));
    const axis=dominant?'x':'z';
    const direction=dominant?Math.sign(Math.cos(angle)):-Math.sign(Math.sin(angle));
    return model(x,z,angle,scale,{
      pivot:[x+Math.sin(angle)*front*scale,y*scale,z+Math.cos(angle)*front*scale],
      axis, amplitude:amount*direction, phase,
    });
  }

  function cow(x,z,angle,s,spot) {
    const b=model(x,z,angle,s);
    const pale='#d9cbb0', hide=spot?'#58433a':'#484740', hooves='#4f4841';
    b(0,1.04,-.16,.79,.66,1.58,pale);
    b(0,1.235,-.15,.72,.329,1.45,'#cfc3a9');
    b(.124,1.248,-.60,.548,.357,.41,hide);
    b(.364,.985,-.56,.094,.37,.537,hide);
    b(-.363,1.104,.11,.102,.39,.55,hide);
    b(-.162,1.343,.38,.297,.13,.51,hide);
    for (const u of [-.264,.263]) {
      for (const v of [-.68,.49]) {
        b(u,.36,v,.176,.71,.195,v<0?hide:pale);
        b(u,.092,v+.018,.188,.173,.218,hooves);
      }
    }
    b(0,.65,-.515,.37,.20,.38,'#a97468');
    const head=animalHead(x,z,angle,s,1.23,.37,spot?.23:.34,spot?1.7:.18);
    head(0,1.237,.55,.514,.58,.49,hide);
    head(0,1.277,.893,.472,.449,.387,pale);
    head(0,1.115,1.101,.477,.233,.19,'#a97d71');
    for (const u of [-.231,.231]) {
      head(u,1.313,1.011,.035,.061,.051,hooves);
      head(u*1.44,1.435,.804,.214,.095,.14,hide);
      head(u*1.24,1.53,.844,.089,.101,.091,'#bda684');
      head(u*.50,1.153,1.202,.056,.046,.031,'#614b45');
    }
    b(-.084,.995,-.96,.083,.58,.104,hide);
    b(-.081,.724,-.972,.111,.161,.112,hooves);
  }

  function sheep(x,z,angle,s,headDown=false) {
    const b=model(x,z,angle,s);
    const wool=['#d6c9aa','#cdc1a5','#e3d6b7','#dad1ba'];
    for (const u of [-.217,.232]) {
      for (const v of [-.40,.33]) {
        b(u,.285,v,.113,.535,.117,'#746550');
        b(u,.064,v+.012,.134,.114,.146,'#5d5247');
      }
    }
    b(0,.631,-.04,.65,.367,1.0,wool[0]);
    b(0,.842,-.08,.56,.316,.81,wool[1]);
    for (const u of [-.275,.277]) {
      for (const v of [-.38,-.12,.15,.40]) {
        b(u,.728+(rand()-.5)*.14,v,.213,.301,.237,choose(wool));
      }
    }
    b(0,.891,-.196,.34,.242,.52,wool[2]);
    b(.051,.656,-.576,.17,.197,.16,wool[3]);
    const y=headDown?.57:.801;
    const head=animalHead(x,z,angle,s,.74,.34,headDown?.22:.53,z*.67);
    head(0,y,.56,.287,.375,.287,'#9d8d70');
    head(0,y-.094,.705,.238,.172,.188,'#81715a');
    head(0,y+.18,.535,.326,.126,.259,wool[2]);
    for (const u of [-.157,.157]) {
      head(u*1.33,y+.081,.552,.224,.077,.106,'#9d8b73');
      head(u,y+.045,.644,.022,.042,.041,'#504639');
    }
  }

  function chicken(x,z,angle,s,colors) {
    const b=model(x,z,angle,s);
    for (const u of [-.08,.08]) {
      b(u,.116,.013,.033,.21,.036,'#b28b4a');
      b(u,.028,.049,.039,.037,.113,'#bb8742');
    }
    b(0,.293,0,.314,.235,.409,colors[0]);
    b(.006,.411,.066,.246,.158,.277,colors[1]);
    const head=animalHead(x,z,angle,s,.46,.095,.32,z*1.12);
    head(0,.497,.157,.153,.221,.158,colors[0]);
    head(0,.585,.237,.129,.119,.103,colors[1]);
    head(0,.566,.311,.058,.053,.102,'#d2a049');
    head(0,.652,.214,.057,.076,.114,'#a94735');
    head(0,.531,.257,.052,.064,.053,'#a84d36');
    b(0,.425,-.21,.102,.181,.21,colors[2]);
    b(0,.507,-.281,.070,.173,.125,colors[2]);
    head(.079,.577,.252,.016,.023,.023,'#473e35');
    head(-.079,.575,.25,.016,.023,.023,'#473e35');
  }

  cow(8.4,1.28,1.42,1.10,false);
  cow(12.3,-1.7,2.33,.89,true);
  sheep(6.77,5.01,.48,.94);
  sheep(9.18,5.42,1.31,.76,true);
  sheep(12.84,4.15,.82,.95);
  sheep(13.39,1.98,-.84,.68,true);
  chicken(-2.3,1.05,1.20,1.23,['#b17846','#c7965c','#72543e']);
  chicken(-1.58,1.72,.20,.97,['#d3bb94','#e0c8a9','#af997e']);
  chicken(-2.65,1.85,2.0,.91,['#6e6552','#978567','#4d5044']);

  const trough=model(7.52,-3.73,.12);
  trough(0,.342,0,2.20,.16,.648,'#78654e');
  trough(0,.432,0,2.14,.056,.541,'#60706a');
  for (const v of [-.34,.34]) trough(0,.479,v,2.21,.241,.073,'#92765a');
  for (const u of [-1.066,1.066]) {
    trough(u,.454,0,.075,.256,.659,'#8c6f54');
    trough(u*.80,.173,0,.127,.346,.532,'#655342');
  }

  // Windpump and stock tank sit along the edge of the distant field.
  const pump=model(12.08,-10.5,0);
  for (const u of [-1,1]) {
    for (const v of [-1,1]) {
      for (let i=0;i<17;i++) {
        const radius=.855-i*.042;
        pump(u*radius,.17+i*.312,v*radius,.107,.34,.105,'#776e5a');
      }
    }
  }
  for (let i=0;i<6;i++) {
    const y=.40+i*.84, r=.80-i*.112;
    for (const u of [-r,r]) pump(u,y,0,.09,.075,2*r,'#8a816a');
    for (const v of [-r,r]) pump(0,y+.018,v,2*r,.075,.09,'#8a816a');
  }
  pump(0,5.447,0,.29,.391,.477,'#857767');
  const blades=model(12.08,-10.5,0,1,{
    pivot:[12.08,5.46,-10.15],axis:'z',amplitude:0,phase:0,spin:.55,
  });
  for(let arm=0;arm<8;arm++) {
    const a=2*Math.PI*arm/8;
    for(let i=0;i<9;i++) {
      const r=.13*i;
      blades(Math.cos(a)*r,5.46+Math.sin(a)*r,.319,.132,.110,.070,'#8d8270');
    }
    for(let i=0;i<5;i++) {
      const r=.124*i, theta=a+.21;
      blades(Math.cos(theta)*(r+.67),5.46+Math.sin(theta)*(r+.67),.348+.003*i,.242,.219,.065,'#a09276');
    }
  }
  pump(0,5.461,.39,.239,.247,.127,'#9d8f75');

  return { chimney: null };
}
