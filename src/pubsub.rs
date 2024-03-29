use crate::block::{Block, Tx};
use std::collections::vec_deque::VecDeque;
use std::collections::HashMap;
use std::fmt::{Debug, Error, Formatter};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use threadpool::ThreadPool;

pub struct Subscription {
    pubsub: PubSub,
    channel_id: String,
    id: u64,
}

impl Debug for Subscription {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        fmt.write_str(&format!("Sub(channel={})", self.channel_id))
    }
}

impl Subscription {
    pub fn id(&self) -> &str {
        &self.channel_id
    }
    pub fn cancel(self) { /* self is dropped */
    }
    pub fn notify_others(&self, msg: &DataEle) {
        self.pubsub
            .notify_exception(&self.channel_id, msg, Some(self.id));
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        self.pubsub.unregister(self);
    }
}

pub struct SubActivator {
    sub: Subscription,
}
impl SubActivator {
    pub fn activate<F>(self, func: F) -> Subscription
    where
        F: FnMut(DataEle) + 'static + Send,
    {
        self.sub
            .pubsub
            .activate(&self.sub.channel_id, self.sub.id, func);
        self.sub
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum DataEle {
    Block(Arc<Block>),
    Tx(Arc<Tx>),
}

impl From<&Block> for DataEle {
    fn from(v: &Block) -> Self {
        Self::Block(Arc::new(v.clone()))
    }
}

impl From<Block> for DataEle {
    fn from(v: Block) -> Self {
        Self::Block(Arc::new(v))
    }
}

impl From<&Tx> for DataEle {
    fn from(v: &Tx) -> Self {
        Self::Tx(Arc::new(v.clone()))
    }
}

impl From<Tx> for DataEle {
    fn from(v: Tx) -> Self {
        Self::Tx(Arc::new(v))
    }
}

struct SubData {
    running: RAIIBool,
    backlog: VecDeque<DataEle>,
    func: Option<Arc<Mutex<Box<dyn FnMut(DataEle) + Send>>>>,
}

struct InnerPubSub {
    channels: HashMap<String, HashMap<u64, SubData>>,
    //id will stay unique for hundreds of years, even at ~1 billion/sec
    next_id: u64,
    thread_pool: Rc<ThreadPool>,
}
#[derive(Clone)]
pub struct PubSub {
    inner: Arc<Mutex<InnerPubSub>>,
}
unsafe impl Send for PubSub {}
unsafe impl Sync for PubSub {}

#[derive(Clone)]
struct RAIIBool {
    value: Arc<Mutex<bool>>,
}
impl RAIIBool {
    fn new(value: bool) -> RAIIBool {
        RAIIBool {
            value: Arc::new(Mutex::new(value)),
        }
    }
    fn set(&self, value: bool) -> bool {
        let mut guard = self.value.lock().unwrap();
        let old: bool = *guard;
        *guard = value;
        old
    }
    fn new_guard(&self, value: bool) -> RAIIBoolGuard {
        RAIIBoolGuard::new(self.clone(), value)
    }
}

struct RAIIBoolGuard {
    data: RAIIBool,
    value: bool,
}
impl RAIIBoolGuard {
    fn new(data: RAIIBool, value: bool) -> RAIIBoolGuard {
        RAIIBoolGuard {
            data: data,
            value: value,
        }
    }
    fn done(self) {}
}
impl Drop for RAIIBoolGuard {
    fn drop(&mut self) {
        self.data.set(self.value);
    }
}

impl PubSub {
    pub fn new_with_pool(tpool: Rc<ThreadPool>) -> PubSub {
        PubSub {
            inner: Arc::new(Mutex::new(InnerPubSub {
                channels: HashMap::new(),
                next_id: 0,
                thread_pool: tpool,
            })),
        }
    }
    pub fn new(num_threads: usize) -> PubSub {
        PubSub {
            inner: Arc::new(Mutex::new(InnerPubSub {
                channels: HashMap::new(),
                next_id: 0,
                thread_pool: Rc::new(ThreadPool::new(num_threads)),
            })),
        }
    }
    fn internal_subscribe<F>(&self, channel: &str, func: Option<F>) -> Subscription
    where
        F: FnMut(DataEle) + 'static + Send,
    {
        let mut data = self.inner.lock().unwrap();
        if !data.channels.contains_key(channel) {
            data.channels.insert(channel.into(), HashMap::new());
        }
        let id = data.next_id;
        data.next_id += 1;
        let sub_data = SubData {
            running: RAIIBool::new(false),
            backlog: VecDeque::new(),
            func: func.map(|f| Arc::new(Mutex::new(Box::new(f) as Box<_>))),
        };

        let subscriptions = data.channels.get_mut(channel).unwrap();
        subscriptions.insert(id, sub_data);
        Subscription {
            pubsub: self.clone(),
            channel_id: channel.into(),
            id: id,
        }
    }
    pub fn subscribe<F>(&self, channel: &str, func: F) -> Subscription
    where
        F: FnMut(DataEle) + 'static + Send,
    {
        self.internal_subscribe(channel, Some(func))
    }

    #[allow(unused_assignments)]
    pub fn lazy_subscribe(&self, channel: &str) -> SubActivator {
        let mut func = Some(|_| {}); //used to give type info to 'func'
        func = None;
        SubActivator {
            sub: self.internal_subscribe(channel, func),
        }
    }
    fn activate<F>(&self, channel: &str, id: u64, func: F)
    where
        F: FnMut(DataEle) + 'static + Send,
    {
        let mut inner = self.inner.lock().unwrap();
        let pool = inner.thread_pool.clone();
        let subs = inner.channels.get_mut(channel).unwrap(); //channel will always exist
        let sub_data = subs.get_mut(&id).unwrap(); //sub id will always exist
        sub_data.func = Some(Arc::new(Mutex::new(Box::new(func))));
        self.schedule_worker(sub_data, channel, id, &pool);
    }
    pub fn num_channels(&self) -> usize {
        let data = self.inner.lock().unwrap();
        data.channels.len()
    }
    fn unregister(&self, sub: &Subscription) {
        let mut inner = self.inner.lock().unwrap();
        let mut remove_channel = false;
        {
            let sub_list = inner.channels.get_mut(&sub.channel_id).unwrap();
            sub_list.remove(&sub.id);
            if sub_list.len() == 0 {
                remove_channel = true;
            }
        }
        if remove_channel {
            inner.channels.remove(&sub.channel_id);
        }
    }
    fn schedule_worker(
        &self,
        sub_data: &mut SubData,
        channel: &str,
        id: u64,
        pool: &Rc<ThreadPool>,
    ) {
        if !sub_data.running.set(true) {
            //if not currently running
            let thread_running = sub_data.running.clone();
            if let Some(func) = sub_data.func.clone() {
                let pubsub = self.clone();
                let channel = channel.to_string();
                let id = id.clone();
                pool.execute(move || {
                    use std::ops::DerefMut;
                    let finish_guard = thread_running.new_guard(false);
                    let mut guard = func.lock().unwrap();
                    let func = guard.deref_mut();
                    let mut running = true;
                    while running {
                        let mut notification_message = None;
                        {
                            let mut inner = pubsub.inner.lock().unwrap();
                            if let Some(subs) = inner.channels.get_mut(&channel) {
                                if let Some(sub_data) = subs.get_mut(&id) {
                                    if let Some(msg) = sub_data.backlog.pop_front() {
                                        notification_message = Some(msg);
                                    }
                                }
                            }
                        }
                        if let Some(msg) = notification_message {
                            func(msg);
                        } else {
                            running = false;
                        }
                    }
                    finish_guard.done();
                });
            } else {
                thread_running.set(false);
            }
        }
    }
    pub fn notify(&self, channel: &str, msg: &DataEle) {
        self.notify_exception(channel, msg, None)
    }
    fn notify_exception(&self, channel: &str, msg: &DataEle, exception: Option<u64>) {
        let mut inner = self.inner.lock().unwrap();
        let pool = inner.thread_pool.clone();
        if let Some(subscriptions) = inner.channels.get_mut(channel) {
            for (id, sub_data) in subscriptions {
                if Some(*id) != exception {
                    sub_data.backlog.push_back(msg.clone());
                    self.schedule_worker(sub_data, channel, *id, &pool);
                }
            }
        }
    }
}

#[test]
fn basic_test() {
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;
    let pubsub = PubSub::new(5);
    let count = Arc::new(Mutex::new(0));
    {
        let count1 = count.clone();
        let sub1 = pubsub.subscribe("channel1", move |_| {
            sleep(Duration::from_millis(1000));
            *count1.lock().unwrap() += 1;
        });
        let count2 = count.clone();
        let sub2 = pubsub.subscribe("channel2", move |_| {
            sleep(Duration::from_millis(1000));
            *count2.lock().unwrap() += 1;
        });
        pubsub.notify("channel1", &Block::default().into());
        pubsub.notify("channel1", &Block::default().into());

        pubsub.notify("channel2", &Block::default().into());
        pubsub.notify("channel2", &Tx::default().into());

        sleep(Duration::from_millis(500));
        assert_eq!(*count.lock().unwrap(), 0);
        sub2.cancel();
        sleep(Duration::from_millis(1000));
        assert_eq!(*count.lock().unwrap(), 2);
        sleep(Duration::from_millis(1000));
        assert_eq!(*count.lock().unwrap(), 3);
        sub1.cancel();
    }
    assert!(pubsub.num_channels() == 0);
}

#[test]
fn lazy_subscribe() {
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;
    let pubsub = PubSub::new(5);
    let count = Arc::new(Mutex::new(0));

    let tx = Arc::new(Tx::default());

    let sub1_activator = pubsub.lazy_subscribe("channel1");
    pubsub.notify("channel1", &DataEle::Tx(tx.clone()));

    let count1 = count.clone();

    let tx = tx.clone();
    let sub1 = sub1_activator.activate(move |msg| {
        assert_eq!(msg, DataEle::Tx(tx.clone()));
        *count1.lock().unwrap() += 1;
    });
    sleep(Duration::from_millis(500));
    assert_eq!(*count.lock().unwrap(), 1);
    sub1.cancel();
}

#[test]
fn notify_exception() {
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;
    let pubsub = PubSub::new(5);
    let count = Arc::new(Mutex::new(0));

    let count1 = count.clone();
    let sub1 = pubsub.subscribe("channel1", move |_| {
        *count1.lock().unwrap() -= 1;
    });
    let count2 = count.clone();
    let sub2 = pubsub.subscribe("channel1", move |_| {
        *count2.lock().unwrap() += 1;
    });

    sub1.notify_others(&Block::default().into());

    sleep(Duration::from_millis(500));
    assert_eq!(*count.lock().unwrap(), 1);
    sub1.cancel();
    sub2.cancel();
}
