// Only this worker loads the reasoner. Playback on the UI thread sends no requests.
import init, { handle_request } from './pkg/nibli_learn_worker.js';
const ready = init();
let queue = Promise.resolve();
self.onmessage = (event) => {
  const request = typeof event.data === 'string' ? JSON.parse(event.data) : event.data;
  queue = queue.then(async () => {
    await ready;
    self.postMessage(JSON.parse(handle_request(JSON.stringify(request))));
  }).catch(error => {
    self.postMessage({ id: request.id, generation: request.generation,
      result: { Err: { kind: 'operational', message: String(error) } } });
  });
};
