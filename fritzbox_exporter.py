#!/usr/bin/env python3

# Copyright 2020 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import collections
import signal
import threading

from absl import app
from absl import flags
from absl import logging

from cachetools import cachedmethod, TTLCache
import fritzconnection
import inflection
from prometheus_client import start_wsgi_server, Counter, Gauge


FLAGS = flags.FLAGS
flags.DEFINE_string('listen', ':9413', 'Address:port to listen on')
flags.DEFINE_string('address', 'fritz.box', 'Hostname/IP address of FritzBox')
flags.DEFINE_string('username', '', 'Username to authenticate with')
flags.DEFINE_string('password', '', 'Password to authenticate with')
flags.DEFINE_bool('verbose', False, 'Enable verbose logging')
flags.DEFINE_bool('ipv6_hack', False, 'Enable IPv6 socketserver hack')
flags.DEFINE_list(
    'service_skiplist',
    [
        'DeviceConfig1',
        'X_AVM-DE_OnTel1',
        'X_AVM-DE_Filelinks1',
        'WANIPConnection1',
    ],
    'Services to skip in enumeration')
flags.DEFINE_list(
    'action_skiplist',
    [
        'GetDefaultWEPKeyIndex',
        'GetLinkLayerMaxBitRates',
    ],
    'Actions to skip in enumeration')


Action = collections.namedtuple('Action', ['service_name', 'action_name', 'arguments'])


def metricify(service, action, variable):
    variable = variable.replace('.', '_')
    return f"fritzbox_{inflection.underscore(variable)}"


def collect_actions(client):
    actions = []
    for service_name, service in client.services.items():
        for action_name, action in service.actions.items():
            # Filter out all state modification actions and those filtering by a search criteria
            if not action_name.startswith('Get'):
                continue
            just_out_parameters = all([arg.direction == 'out' for arg in action.arguments.values()])
            if not just_out_parameters:
                continue
            arguments = {}
            for argument_name, argument in action.arguments.items():
                arguments[argument_name] = service.state_variables[argument.relatedStateVariable]
            actions.append(Action(service_name, action_name, arguments))
    return actions


cache = TTLCache(maxsize=200, ttl=10)

def hashkey(*args, **kwargs):
    return args[0].service_name, args[0].action_name


@cachedmethod(cache=lambda _: cache, key=hashkey)
def call_action(client, action):
    return client.call_action(action.service_name, action.action_name)


def handle_read(client, action, key):
    output = call_action(client, action)
    return output[key]


def collect_variables(client, actions):
    variables = {}
    for action in actions:
        if action.service_name in FLAGS.service_skiplist:
            continue
        if action.action_name in FLAGS.action_skiplist:
            continue
        logging.debug(f'Calling service {action.service_name}, action {action.action_name}')
        output = call_action(client, action)
        for key in output.keys():
            state_variable = action.arguments[key]
            name = state_variable.name
            logging.debug(f'Detected variable: {action.service_name} {action.action_name} {name}')
            metric_name = metricify(action.service_name, action.action_name, name)
            metric = None
            if ((name.endswith('Rate') and not name.startswith('Max') and not name.startswith('Min'))
                or name.endswith('Sent') or name.endswith('Received') or 'Total' in name
                or 'Attenuation' in name or 'Margin' in name or name.endswith('Errors')):
                metric = (variables[metric_name]
                          if metric_name in variables
                          else Gauge(metric_name, '', labelnames=('service', 'action')))
                variables[metric_name] = metric
            if metric:
                metric.labels(service=action.service_name,
                              action=action.action_name).set_function(
                    lambda action=action, key=key: handle_read(client, action, key)
                )
                if FLAGS.verbose:
                    data_type = action.arguments[key].dataType
                    result = handle_read(client, action, key)
                    logging.debug(f'{action.service_name} {action.action_name} {name} '
                        '{data_type} {result}')
    return variables


exit = threading.Event()


def quit(unused_signo, unused_frame):
    exit.set()


def main(unused_argv):
    if FLAGS.verbose:
        logging.set_verbosity(logging.DEBUG)
    if FLAGS.ipv6_hack:
        # TODO(pkern): Eliminate this. Right now socketserver is IPv4-only,
        # so what we should do is actually bring up a proper HTTP server and
        # then map the path. However to enable IPv6 listening this is an
        # awful hack of monkey-patching to make it work for now.
        import socketserver
        import socket
        socketserver.TCPServer.address_family = socket.AF_INET6
    client = fritzconnection.FritzConnection(
        address=FLAGS.address,
        user=FLAGS.username,
        password=FLAGS.password,
    )
    logging.info(f'Connection succeeded to {client.modelname} on {FLAGS.address}')
    actions = collect_actions(client)
    logging.info(f'Collected {len(actions)} actions')
    variables = collect_variables(client, actions)
    logging.info(f'Collected {len(variables)} variables')
    address, port = FLAGS.listen.rsplit(':', 1)
    start_wsgi_server(port=int(port), addr=address)
    logging.info(f'Listening on {FLAGS.listen}')
    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(sig, quit)
    exit.wait()


if __name__ == '__main__':
    app.run(main)
