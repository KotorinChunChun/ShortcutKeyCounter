# Power lifecycle test
Local/Elevatedの双方で Sleep、Hibernate、Lock、Unlock、Resume を確認する。復帰後に計測ONを維持し、Elevated helperが失われた場合はUAC再表示なしでLocal fallbackすることを確認する。
