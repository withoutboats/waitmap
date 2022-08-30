use std::sync::Arc;
use std::time::Duration;

use waitmap::WaitMap;

use async_std::task;
use async_std::task::sleep;

#[test]
fn works_like_a_normal_map() {
    let map = WaitMap::new();
    assert!(map.get("Rosa Luxemburg").is_none());
    map.insert(String::from("Rosa Luxemburg"), 0);
    assert_eq!(map.get("Rosa Luxemburg").unwrap().value(), &0);
    assert!(map.get("Voltairine de Cleyre").is_none());
}

#[test]
fn simple_waiting() {
    let map: Arc<WaitMap<String, i32>> = Arc::new(WaitMap::new());
    let map2 = map.clone();

    let handle = task::spawn(async move {
        let rosa = map.wait("Rosa Luxemburg").await;
        assert_eq!(rosa.unwrap().value(), &0);
        assert!(map.wait("Voltairine de Cleyre").await.is_none());
    });

    task::spawn(async move {
        task::sleep(Duration::from_millis(140)).await;
        map2.insert(String::from("Rosa Luxemburg"), 0);
        task::sleep(Duration::from_millis(140)).await;
        map2.cancel("Voltairine de Cleyre");
    });

    task::block_on(handle);
}

#[test]
fn simple_waiting_mut() {
    let map: Arc<WaitMap<String, i32>> = Arc::new(WaitMap::new());
    let map2 = map.clone();

    let handle = task::spawn(async move {
        let rosa = map.wait_mut("Rosa Luxemburg").await;
        assert_eq!(rosa.unwrap().value(), &0);
        assert!(map.wait_mut("Voltairine de Cleyre").await.is_none());
    });

    task::spawn(async move {
        task::sleep(Duration::from_millis(140)).await;
        map2.insert(String::from("Rosa Luxemburg"), 0);
        task::sleep(Duration::from_millis(140)).await;
        map2.cancel("Voltairine de Cleyre");
    });

    task::block_on(handle);
}

#[test]
fn cancel_all_cancels_all() {
    let map: Arc<WaitMap<String, i32>> = Arc::new(WaitMap::new());
    let map2 = map.clone();

    let handle = task::spawn(async move {
        let rosa = map.wait("Rosa Luxemburg");
        let voltairine = map.wait("Voltairine de Cleyre");
        assert!(rosa.await.is_none());
        assert!(voltairine.await.is_none());
    });

    task::spawn(async move {
        task::sleep(Duration::from_millis(140)).await;
        map2.cancel_all();
    });

    task::block_on(handle);
}

#[test]
fn multiple_tasks_can_wait_one_key() {
    let map: Arc<WaitMap<String, i32>> = Arc::new(WaitMap::new());
    let map1 = map.clone();
    let map2 = map.clone();

    task::spawn(async move {
        map.insert(String::from("Rosa Luxemburg"), 0);
    });

    let handle1 = task::spawn(async move {
        let rosa = map1.wait("Rosa Luxemburg").await;
        assert_eq!(rosa.unwrap().value(), &0);
    });

    let handle2 = task::spawn(async move {
        let rosa = map2.wait("Rosa Luxemburg").await;
        assert_eq!(rosa.unwrap().value(), &0);
    });

    task::block_on(handle1);
    task::block_on(handle2);
}

#[test]
fn single_remove_works_like_normal_maps() {
    let test = async {
        let map = WaitMap::new();
        map.insert(String::from("Rosa Luxemburg"), 0);

        let rosa = map.remove_wait("Rosa Luxemburg").await;
        assert_eq!(rosa.unwrap(), (String::from("Rosa Luxemburg"), 0));
    };

    task::block_on(test);
}


#[test]
fn only_one_remove_gets_value() {
    let map: Arc<WaitMap<String, i32>> = Arc::new(WaitMap::new());
    let map1 = map.clone();
    let map2 = map.clone();
    let map3 = map.clone();
    let map4 = map.clone();


    let handle1 = task::spawn(async move { map1.remove_wait("Rosa Luxemburg").await });
    let handle2 = task::spawn(async move { map2.remove_wait("Rosa Luxemburg").await });
    let handle3 = task::spawn(async move { map3.remove_wait("Rosa Luxemburg").await });
    let handle4 = task::spawn(async move { map4.remove_wait("Rosa Luxemburg").await });


    task::block_on(async move {
        sleep(Duration::from_millis(140)).await;
        map.insert(String::from("Rosa Luxemburg"), 0);
    });

    let returned = task::block_on(async move {
        let remove1 = handle1.await;
        let remove2 = handle2.await;
        let remove3 = handle3.await;
        let remove4 = handle4.await;

        vec![remove1, remove2, remove3, remove4]
    });

    // we don't particularly care which one beat the rest and go the value, but there must be one
    // all the rest are None
    let some_count = returned
        .iter()
        .filter(|o| o.is_some())
        .count();
    assert_eq!(some_count, 1);

    // and the None count better be total - 1
    let none_count = returned
        .iter()
        .filter(|r| r.is_none())
        .count();
    assert_eq!(none_count, returned.len() - 1);

    // and let's makes sure the one that beat the beat actually got the value
    let (key, value) = returned
        .into_iter()
        .filter(|r| r.is_some())
        .next()
        .expect("there should be one Some")
        .expect("the Some should have a value");

    assert_eq!((key, value), (String::from("Rosa Luxemburg"), 0));
}
