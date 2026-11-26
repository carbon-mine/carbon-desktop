/** Low banks of stepped fair-weather clouds, assembled in contiguous columns. */
export function addClouds(box, id) {
  const settings = {
    farmland: [-.50, 0, 0, '#ffe1c9'],
    riverside: [-.51, 1, 1, '#ffe9ce'],
    alpine: [-.30, 2, 8, '#eacdde'],
    marsh: [-.37, 3, 2, '#dfc7e0'],
    orchard: [-.41, 4, 1, '#ffe0bf'],
  };
  const [yaw, offset, lift, tint] = settings[id];
  const banks = [[-35,61,12,1.55],[9,79,17,2.3],[48,88,12,1.25]];
  const profiles=[[1,2,2,1,2,4,3,2,1],[1,1,2,3,3,2,2,1,1],[1,2,2,3,2,1]];
  for(let n=0;n<banks.length;n++){
    const [side, distance, altitude, scale]=banks[n];
    const x=side*Math.cos(yaw)+distance*Math.sin(yaw);
    const z=side*-Math.sin(yaw)-distance*Math.cos(yaw);
    const heights=profiles[(n+offset)%profiles.length];
    for(let i=0;i<heights.length;i++){
      const j=(offset+n)%2?heights.length-1-i:i;
      const h=heights[j]*scale*.31, step=(i-4)*scale;
      const base=altitude+lift+(j<2||j>6?.26:0);
      box(x+step,base+h/2,z+((j===0||j===8)?scale*.45:0),scale,h,scale*(1.6+.24*heights[j]),tint);
    }
  }
}
