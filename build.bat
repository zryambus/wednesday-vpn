docker build -t builder . 
docker build -t ivolchenkov/wgc -f ./wireguard_control.dockerfile .
docker build -t ivolchenkov/wgb -f ./telegram_bot.dockerfile .