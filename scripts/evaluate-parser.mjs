import {execFileSync,spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import path from 'node:path';
const [root,parent,head,tools,out]=process.argv.slice(2);
if (![root,parent,head,tools,out].every(Boolean)) throw Error('root parent head tools output required');
const fw=await import(path.join(tools,'node_modules/@metaharness/flywheel/dist/index.js'));
const git=(...a)=>execFileSync('git',a,{cwd:root,encoding:'utf8'}).trim();
const probe=git('show',head+':scripts/parser-boundary.cjs');
if(probe!==git('show',parent+':scripts/parser-boundary.cjs'))throw Error('fixture changed');
if(git('show',head+':package-lock.json')!==git('show',parent+':package-lock.json'))throw Error('lock changed');
const inverse=git('show',parent+':index.js');
const env={...process.env,CARGO_TARGET_DIR:'/tmp/schema-oct28c-target',CARGO_INCREMENTAL:'0',CARGO_PROFILE_DEV_DEBUG:'0',CARGO_PROFILE_TEST_DEBUG:'0'};
const run=(c,a,s)=>spawnSync('timeout',['--kill-after=5',String(s),c,...a],{cwd:root,env,encoding:'utf8',timeout:(s+10)*1000,maxBuffer:3000000});
const measurements={};
const evaluate=async(policy,suite)=>{
  if(![parent,head].includes(policy.revision))throw Error('unexpected revision');
  if(!measurements[policy.revision]){
    git('switch','--detach',policy.revision);
    const started=performance.now();
    const p=run('node',['-e',probe,'parser-probe',root],20);
    if(p.status!==0)throw Error('probe execution failed: '+p.stderr);
    const observed=JSON.parse(p.stdout);
    const node=run('node',['test.js'],30);
    const workspace=run('cargo',['test','--locked','--offline','--workspace','-j','2'],180);
    const audit=run('npm',['audit','--omit=dev','--audit-level=high','--json'],35);
    measurements[policy.revision]={...observed,nodeExit:node.status,workspaceExit:workspace.status,auditExit:audit.status,
      nodeOutput:node.stdout+node.stderr,workspaceOutput:workspace.stdout+workspace.stderr,auditOutput:audit.stdout+audit.stderr,
      elapsedSeconds:(performance.now()-started)/1000};
  }
  const m=measurements[policy.revision],items=suite.id==='anchors'?m.anchors:m.holdout;
  const wins=items.filter(Boolean).length;
  return{primary:wins/items.length,noopRate:1-wins/items.length,costPerWin:m.elapsedSeconds/Math.max(1,wins),itemWins:items,
    regressed:!m.anchors.every(Boolean)||[m.nodeExit,m.workspaceExit,m.auditExit].some(x=>x!==0)};
};
try{
  const r=await fw.runFlywheelGenerations({rootPolicy:{revision:parent},proposer:async()=>({value:head,summary:'Preserve parser client error status',
    inverse:{path:'index.js',parentBytes:inverse,hash:createHash('sha256').update(inverse).digest('hex')}}),
    evaluator:evaluate,promotionRule:fw.meetsPromotionRule,holdout:{id:'holdout',items:[0,1,2]},anchor:{id:'anchors',items:[0,1,2,3,4,5]},
    mutationTargets:['revision'],maxGenerations:1,signer:fw.makeSigner(),dataSource:'LIVE'});
  const replay=fw.verifyReplayBundle(r.replayBundle,{pinnedGateFingerprint:fw.gateFingerprint(fw.meetsPromotionRule)});
  writeFileSync(out,JSON.stringify({parent,head,measurements,finalPolicy:r.finalPolicy,replay,replayBundle:r.replayBundle},null,2)+'\n');
  console.log(JSON.stringify({replay:replay.pass,finalPolicy:r.finalPolicy,verdicts:r.replayBundle.all_commits.map(c=>({verdict:c.verdict,reasons:c.failureReasons}))}));
}finally{git('switch','--detach',head);}
