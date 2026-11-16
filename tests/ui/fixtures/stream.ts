import type { Page } from '@playwright/test';

// Synthetic only. Explicit packets let tests exercise rendering independently
// of provider timing, without sending a model request.
export async function streamFixture(page: Page, history = 0) {
  await page.route('**/src/backend.ts*', route => route.fulfill({ contentType: 'application/javascript', body: `
    import {mockBackend} from './mock-backend.ts';
    export const nativeApp=false;
    const chat={id:'stream-fixture',title:'Synthetic streaming',folderId:null,updatedAt:1};
    const sources=[{id:'ref',kind:'web',title:'Synthetic reference',url:'https://example.com',chatId:null,messageId:null,snippet:'Reference details. '.repeat(100)}];
    const messages=Array.from({length:${history}},(_,i)=>({id:'history-'+i,chatId:chat.id,role:'assistant',content:Array.from({length:5},(_,j)=>'Paragraph '+i+'.'+j+' — '+ 'Synthetic reading text. '.repeat(14)).join('\\n\\n'),status:'complete',sources:i===${history}-1?sources:[],createdAt:i}));
    const answer={id:'stream-answer',chatId:chat.id,role:'assistant',content:'',status:'streaming',sources:[],createdAt:100};
    messages.push(answer);
    let listener;
    window.fixtureStream=(kind,text='')=>{
      if(kind==='delta') answer.content+=text;
      if(kind==='done')answer.status='complete';
      if(kind==='stopped')answer.status='stopped';
      if(kind==='error')answer.status='error';
      listener?.({kind,text,chatId:chat.id,messageId:answer.id});
    };
    export const backend={
      async invoke(command,args={}) {
        if(command==='bootstrap') {const data=await mockBackend.invoke(command,args);return {...data,chats:[chat]};}
        if(command==='messages') return structuredClone(messages);
        if(command==='stop_chat'){window.fixtureStream('stopped');return;}
        return mockBackend.invoke(command,args);
      },
      async listen(event,callback){if(event==='chat-stream'){listener=callback;return()=>{listener=undefined;};}return mockBackend.listen(event,callback);}
    };
  ` }));
  await page.goto('/');
  await page.locator('#message-stream-answer').waitFor();
}

export async function packet(page: Page, kind: string, text = '') {
  await page.evaluate(({kind,text}) => (window as unknown as {fixtureStream:(kind:string,text:string)=>void}).fixtureStream(kind,text), {kind,text});
}
