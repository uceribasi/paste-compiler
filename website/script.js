"use strict";
const words=JSON.parse(document.getElementById("demo-copy").textContent);
const input=document.getElementById("demo-input");
const output=document.getElementById("demo-output");
const copyButton=document.getElementById("copy-output");
const statusText=document.getElementById("demo-status");
const note=document.getElementById("demo-validation");
const bench=document.querySelector(".workbench");
const formatButtons=Array.from(document.querySelectorAll("[data-format]"));
const samples={people:"id,name,role\n007,Ada,design\n008,Can,dev\n009,Mina,ops",products:'sku,product,price\nP-01,"Notebook, A5",12.5\nP-02,Desk lamp,39\nP-03,Pen set,7'};
let format="json";

function parseDelimited(text){
 if(text.length>20000)throw new Error(words.limit);
 if(!text.trim())throw new Error(words.empty);
 const first=text.split(/\r?\n/)[0];
 let inQuotes=false,commas=0,tabs=0;
 for(let i=0;i<first.length;i++){
  if(first[i]==='"'){if(inQuotes&&first[i+1]==='"'){i++;continue;}inQuotes=!inQuotes;}
  else if(!inQuotes){if(first[i]===",")commas++;if(first[i]==="\t")tabs++;}
 }
 const delimiter=tabs>commas?"\t":",";
 const rows=[];let row=[],cell="",quoted=false,closed=false;
 for(let i=0;i<text.length;i++){
  const ch=text[i];
  if(quoted){if(ch==='"'){if(text[i+1]==='"'){cell+='"';i++;}else{quoted=false;closed=true;}}else{cell+=ch;}continue;}
  if(ch==='"'){if(cell.length||closed)throw new Error(words.quotes);quoted=true;continue;}
  if(ch===delimiter){row.push(cell);cell="";closed=false;continue;}
  if(ch==="\n"||ch==="\r"){
   row.push(cell);rows.push(row);row=[];cell="";closed=false;
   if(ch==="\r"&&text[i+1]==="\n")i++;
   continue;
  }
  if(closed)throw new Error(words.quotes);
  cell+=ch;
 }
 if(quoted)throw new Error(words.quotes);
 if(cell!==""||row.length||closed){row.push(cell);rows.push(row);}
 while(rows.length&&rows[rows.length-1].length===1&&rows[rows.length-1][0]==="")rows.pop();
 if(rows.length<2)throw new Error(words.empty);
 if(rows.length>201)throw new Error(words.limit);
 const headers=rows.shift();
 if(headers.some(h=>!h.trim())||new Set(headers).size!==headers.length)throw new Error(words.headers);
 if(rows.some(r=>r.length!==headers.length))throw new Error(words.ragged);
 return{headers,rows};
}
function cellValue(s){
 if(s==="true")return true;
 if(s==="false")return false;
 if(s.trim()===s&&s!==""&&Number.isFinite(Number(s))&&String(Number(s))===s)return Number(s);
 return s;
}
function renderData(data,kind){
 const h=data.headers,r=data.rows;
 if(kind==="json")return JSON.stringify(r.map(cells=>Object.fromEntries(h.map((key,i)=>[key,cellValue(cells[i])]))),null,2);
 if(kind==="sql"){
  const ident=s=>'"'+s.replaceAll('"','""')+'"';
  const literal=s=>{const value=cellValue(s);return typeof value==="number"?String(value):typeof value==="boolean"?(value?"TRUE":"FALSE"):"'"+s.replaceAll("'","''")+"'";};
  return 'INSERT INTO "data" ('+h.map(ident).join(", ")+")\nVALUES\n"+r.map(cells=>"  ("+cells.map(literal).join(", ")+")").join(",\n")+";";
 }
 if(kind==="markdown"){
  const escape=s=>s.replaceAll("\\","\\\\").replaceAll("|","\\|").replaceAll("<","&lt;").replaceAll(">","&gt;").replaceAll("\r\n","<br>").replaceAll("\n","<br>").replaceAll("\r","<br>");
  const line=cells=>"| "+cells.map(escape).join(" | ")+" |";
  return[line(h),"| "+h.map(()=>"---").join(" | ")+" |",...r.map(line)].join("\n");
 }
 const quote=s=>/[\t\n\r"]/.test(s)?'"'+s.replaceAll('"','""')+'"':s;
 return[h,...r].map(cells=>cells.map(quote).join("\t")).join("\n");
}
function refresh(){
 try{
  const data=parseDelimited(input.value);
  output.textContent=renderData(data,format);
  statusText.textContent=words.ready+" · "+data.rows.length+" "+words.rows+" / "+data.headers.length+" "+words.columns;
  note.textContent=format==="sql"?words.sql_note:words.demo_note;
  bench.classList.remove("invalid");copyButton.disabled=false;
 }catch(error){output.textContent=error.message;statusText.textContent=words.invalid;note.textContent="";bench.classList.add("invalid");copyButton.disabled=true;}
}
function selectFormat(button){
 format=button.dataset.format;
 formatButtons.forEach(b=>{const selected=b===button;b.setAttribute("aria-selected",String(selected));b.tabIndex=selected?0:-1;});
 document.getElementById("output-panel").setAttribute("aria-labelledby",button.id);
 refresh();
}
formatButtons.forEach((button,index)=>{
 button.addEventListener("click",()=>selectFormat(button));
 button.addEventListener("keydown",event=>{
  let next=index;
  if(event.key==="ArrowRight")next=(index+1)%formatButtons.length;
  else if(event.key==="ArrowLeft")next=(index+formatButtons.length-1)%formatButtons.length;
  else if(event.key==="Home")next=0;
  else if(event.key==="End")next=formatButtons.length-1;
  else return;
  event.preventDefault();selectFormat(formatButtons[next]);formatButtons[next].focus();
 });
});
document.querySelectorAll("[data-sample]").forEach(button=>button.addEventListener("click",()=>{
 input.value=samples[button.dataset.sample];
 document.querySelectorAll("[data-sample]").forEach(b=>b.setAttribute("aria-pressed",String(b===button)));
 refresh();
}));
input.addEventListener("input",()=>{document.querySelectorAll("[data-sample]").forEach(b=>b.setAttribute("aria-pressed","false"));refresh();});
let toastTimer;
function announce(message){const toast=document.getElementById("toast");toast.textContent=message;toast.classList.add("visible");clearTimeout(toastTimer);toastTimer=setTimeout(()=>toast.classList.remove("visible"),3000);}
copyButton.addEventListener("click",async()=>{
 try{
  if(navigator.clipboard&&window.isSecureContext)await navigator.clipboard.writeText(output.textContent);
  else{
   const helper=document.createElement("textarea");helper.value=output.textContent;helper.setAttribute("readonly","");helper.style.cssText="position:fixed;left:-9999px;top:0";document.body.appendChild(helper);helper.select();
   const copied=document.execCommand("copy");helper.remove();copyButton.focus();if(!copied){announce(words.copy_fail);return;}
  }
  announce(words.copied);
 }catch(_){announce(words.copy_fail);}
});
const menuButton=document.querySelector(".menu-button");
const menu=document.getElementById("main-nav");
function closeMenu(){menu.classList.remove("open");menuButton.setAttribute("aria-expanded","false");}
menuButton.addEventListener("click",()=>{const expanded=menuButton.getAttribute("aria-expanded")==="true";menu.classList.toggle("open",!expanded);menuButton.setAttribute("aria-expanded",String(!expanded));});
menu.querySelectorAll("a").forEach(a=>a.addEventListener("click",closeMenu));
document.addEventListener("keydown",event=>{if(event.key==="Escape"&&menu.classList.contains("open")){closeMenu();menuButton.focus();}});
refresh();
