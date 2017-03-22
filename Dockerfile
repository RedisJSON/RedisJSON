FROM sonarsource/local-travis

RUN apt-get -y update && apt-get install -y build-essential cmake python python-pip
RUN pip install redis
