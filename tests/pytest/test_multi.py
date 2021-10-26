# -*- coding: utf-8 -*-

import sys
import os
import redis
import json
from RLTest import Env
from includes import *

from RLTest import Defaults

Defaults.decode_responses = True

# ----------------------------------------------------------------------------------------------

# Path to JSON test case files
HERE = os.path.abspath(os.path.dirname(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../.."))
TESTS_ROOT = os.path.abspath(os.path.join(HERE, ".."))
JSON_PATH = os.path.join(TESTS_ROOT, 'files')

nested_large_key = r'{"jkra":[154,4472,[8567,false,363.84,5276,"ha","rizkzs",93],false],"hh":20.77,"mr":973.217,"ihbe":[68,[true,{"lqe":[486.363,[true,{"mp":{"ory":"rj","qnl":"tyfrju","hf":null},"uooc":7418,"xela":20,"bt":7014,"ia":547,"szec":68.73},null],3622,"iwk",null],"fepi":19.954,"ivu":{"rmnd":65.539,"bk":98,"nc":"bdg","dlb":{"hw":{"upzz":[true,{"nwb":[4259.47],"nbt":"yl"},false,false,65,[[[],629.149,"lvynqh","hsk",[],2011.932,true,[]],null,"ymbc",null],"aj",97.425,"hc",58]},"jq":true,"bi":3333,"hmf":"pl","mrbj":[true,false]}},"hfj":"lwk","utdl":"aku","alqb":[74,534.389,7235,[null,false,null]]},null,{"lbrx":{"vm":"ubdrbb"},"tie":"iok","br":"ojro"},70.558,[{"mmo":null,"dryu":null}]],true,null,false,{"jqun":98,"ivhq":[[[675.936,[520.15,1587.4,false],"jt",true,{"bn":null,"ygn":"cve","zhh":true,"aak":9165,"skx":true,"qqsk":662.28},{"eio":9933.6,"agl":null,"pf":false,"kv":5099.631,"no":null,"shly":58},[null,["uiundu",726.652,false,94.92,259.62,{"ntqu":null,"frv":null,"rvop":"upefj","jvdp":{"nhx":[],"bxnu":{},"gs":null,"mqho":null,"xp":65,"ujj":{}},"ts":false,"kyuk":[false,58,{},"khqqif"]},167,true,"bhlej",53],64,{"eans":"wgzfo","zfgb":431.67,"udy":[{"gnt":[],"zeve":{}},{"pg":{},"vsuc":{},"dw":19,"ffo":"uwsh","spk":"pjdyam","mc":[],"wunb":{},"qcze":2271.15,"mcqx":null},"qob"],"wo":"zy"},{"dok":null,"ygk":null,"afdw":[7848,"ah",null],"foobar":3.141592,"wnuo":{"zpvi":{"stw":true,"bq":{},"zord":true,"omne":3061.73,"bnwm":"wuuyy","tuv":7053,"lepv":null,"xap":94.26},"nuv":false,"hhza":539.615,"rqw":{"dk":2305,"wibo":7512.9,"ytbc":153,"pokp":null,"whzd":null,"judg":[],"zh":null},"bcnu":"ji","yhqu":null,"gwc":true,"smp":{"fxpl":75,"gc":[],"vx":9352.895,"fbzf":4138.27,"tiaq":354.306,"kmfb":{},"fxhy":[],"af":94.46,"wg":{},"fb":null}},"zvym":2921,"hhlh":[45,214.345],"vv":"gqjoz"},["uxlu",null,"utl",64,[2695],[false,null,["cfcrl",[],[],562,1654.9,{},null,"sqzud",934.6],{"hk":true,"ed":"lodube","ye":"ziwddj","ps":null,"ir":{},"heh":false},true,719,50.56,[99,6409,null,4886,"esdtkt",{},null],[false,"bkzqw"]],null,6357],{"asvv":22.873,"vqm":{"drmv":68.12,"tmf":140.495,"le":null,"sanf":[true,[],"vyawd",false,76.496,[],"sdfpr",33.16,"nrxy","antje"],"yrkh":662.426,"vxj":true,"sn":314.382,"eorg":null},"bavq":[21.18,8742.66,{"eq":"urnd"},56.63,"fw",[{},"pjtr",null,"apyemk",[],[],false,{}],{"ho":null,"ir":124,"oevp":159,"xdrv":6705,"ff":[],"sx":false},true,null,true],"zw":"qjqaap","hr":{"xz":32,"mj":8235.32,"yrtv":null,"jcz":"vnemxe","ywai":[null,564,false,"vbr",54.741],"vw":82,"wn":true,"pav":true},"vxa":881},"bgt","vuzk",857]]],null,null,{"xyzl":"nvfff"},true,13],"npd":null,"ha":[["du",[980,{"zdhd":[129.986,["liehns",453,{"fuq":false,"dxpn":{},"hmpx":49,"zb":"gbpt","vdqc":null,"ysjg":false,"gug":7990.66},"evek",[{}],"dfywcu",9686,null]],"gpi":{"gt":{"qe":7460,"nh":"nrn","czj":66.609,"jwd":true,"rb":"azwwe","fj":{"csn":true,"foobar":1.61803398875,"hm":"efsgw","zn":"vbpizt","tjo":138.15,"teo":{},"hecf":[],"ls":false}},"xlc":7916,"jqst":48.166,"zj":"ivctu"},"jl":369.27,"mxkx":null,"sh":[true,373,false,"sdis",6217,{"ernm":null,"srbo":90.798,"py":677,"jgrq":null,"zujl":null,"odsm":{"pfrd":null,"kwz":"kfvjzb","ptkp":false,"pu":null,"xty":null,"ntx":[],"nq":48.19,"lpyx":[]},"ff":null,"rvi":["ych",{},72,9379,7897.383,true,{},999.751,false]},true],"ghe":[24,{"lpr":true,"qrs":true},true,false,7951.94,true,2690.54,[93,null,null,"rlz",true,"ky",true]],"vet":false,"olle":null},"jzm",true],null,null,19.17,7145,"ipsmk"],false,{"du":6550.959,"sps":8783.62,"nblr":{"dko":9856.616,"lz":{"phng":"dj"},"zeu":766,"tn":"dkr"},"xa":"trdw","gn":9875.687,"dl":null,"vuql":null},{"qpjo":null,"das":{"or":{"xfy":null,"xwvs":4181.86,"yj":206.325,"bsr":["qrtsh"],"wndm":{"ve":56,"jyqa":true,"ca":null},"rpd":9906,"ea":"dvzcyt"},"xwnn":9272,"rpx":"zpr","srzg":{"beo":325.6,"sq":null,"yf":null,"nu":[377,"qda",true],"sfz":"zjk"},"kh":"xnpj","rk":null,"hzhn":[null],"uio":6249.12,"nxrv":1931.635,"pd":null},"pxlc":true,"mjer":false,"hdev":"msr","er":null},"ug",null,"yrfoix",503.89,563],"tcy":300,"me":459.17,"tm":[134.761,"jcoels",null],"iig":945.57,"ad":"be"},"ltpdm",null,14.53],"xi":"gxzzs","zfpw":1564.87,"ow":null,"tm":[46,876.85],"xejv":null}'

# FIXME: Test all multi-path options (dot notation and bracket notation):
#  Recursive descent, e.g., $..leaf_val
#  Wildcard (in key and in index), e.g., $.*[*]
#  Array slice [start:end:step], e.g., $.arr[2:4]
#  Union, e.g., $.arr[1,2,4] and  $.[field1, field5]
#  Boolean filter, e.g., $.arr[?(@.field>3 && @.id==null)]

def testDelCommand(env):
    """Test REJSON.DEL command"""
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a": 1, "nested": {"a": 2, "b": 3}}'))
    res = r.execute_command('JSON.DEL', 'doc1', '$..a')
    r.assertEqual(res, 2)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(res, '[{"nested":{"b":3}}]')

    # Test deletion of nested hierarchy - only higher hierarchy is deleted
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$', '{"a": {"a": 2, "b": 3}, "b": ["a", "b"], "nested": {"b":[true, "a","b"]}}'))
    res = r.execute_command('JSON.DEL', 'doc2', '$..a')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc2', '$')
    r.assertEqual(res, '[{"nested":{"b":[true,"a","b"]},"b":["a","b"]}]')

    r.assertOk(r.execute_command('JSON.SET', 'doc3', '$', '[{"ciao":["non ancora"],"nested":[{"ciao":[1,"a"]}, {"ciao":[2,"a"]}, {"ciaoc":[3,"non","ciao"]}, {"ciao":[4,"a"]}, {"e":[5,"non","ciao"]}]}]'))
    res = r.execute_command('JSON.DEL', 'doc3', '$.[0]["nested"]..ciao')
    r.assertEqual(res, 3)
    res = r.execute_command('JSON.GET', 'doc3', '$')
    r.assertEqual(res, '[[{"ciao":["non ancora"],"nested":[{},{},{"ciaoc":[3,"non","ciao"]},{},{"e":[5,"non","ciao"]}]}]]')

    # Test default path
    res = r.execute_command('JSON.DEL', 'doc3')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc3', '$')
    r.assertEqual(res, None)

    # Test missing key
    res = r.execute_command('JSON.DEL', 'non_existing_doc', '..a')
    r.assertEqual(res, 0)

def testForgetCommand(env):
    """Test REJSON.FORGET command"""
    """Alias of REJSON.DEL"""
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a": 1, "nested": {"a": 2, "b": 3}}'))
    res = r.execute_command('JSON.FORGET', 'doc1', '$..a')
    r.assertEqual(res, 2)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(res, '[{"nested":{"b":3}}]')

    # Test deletion of nested hierarchy - only higher hierarchy is deleted
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$', '{"a": {"a": 2, "b": 3}, "b": ["a", "b"], "nested": {"b":[true, "a","b"]}}'))
    res = r.execute_command('JSON.FORGET', 'doc2', '$..a')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc2', '$')
    r.assertEqual(res, '[{"nested":{"b":[true,"a","b"]},"b":["a","b"]}]')

    r.assertOk(r.execute_command('JSON.SET', 'doc3', '$', '[{"ciao":["non ancora"],"nested":[{"ciao":[1,"a"]}, {"ciao":[2,"a"]}, {"ciaoc":[3,"non","ciao"]}, {"ciao":[4,"a"]}, {"e":[5,"non","ciao"]}]}]'))
    res = r.execute_command('JSON.FORGET', 'doc3', '$.[0]["nested"]..ciao')
    r.assertEqual(res, 3)
    res = r.execute_command('JSON.GET', 'doc3', '$')
    r.assertEqual(res, '[[{"ciao":["non ancora"],"nested":[{},{},{"ciaoc":[3,"non","ciao"]},{},{"e":[5,"non","ciao"]}]}]]')

    # Test default path
    res = r.execute_command('JSON.FORGET', 'doc3')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc3', '$')
    r.assertEqual(res, None)

    # Test missing key
    res = r.execute_command('JSON.FORGET', 'non_existing_doc', '..a')
    r.assertEqual(res, 0)


def testSetAndGetCommands(env):
    """Test REJSON.SET command"""
    r = env
    # Test set and get on large nested key
    r.assertIsNone(r.execute_command('JSON.SET', 'doc1', '$', nested_large_key, 'XX'))
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', nested_large_key, 'NX'))
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(res, '[' + nested_large_key + ']')
    r.assertIsNone(r.execute_command('JSON.SET', 'doc1', '$', nested_large_key, 'NX'))
    # Test single path
    res = r.execute_command('JSON.GET', 'doc1', '$..tm')
    r.assertEqual(res, '[[46,876.85],[134.761,"jcoels",null]]')

    # Test multi get and set
    res = r.execute_command('JSON.GET', 'doc1', '$..foobar')
    r.assertEqual(res, '[3.141592,1.61803398875]')
    # Set multi existing values
    res = r.execute_command('JSON.SET', 'doc1', '$..foobar', '"new_val"')
    res = r.execute_command('JSON.GET', 'doc1', '$..foobar')
    r.assertEqual(res, '["new_val","new_val"]')

    # Test multi set and get on small nested key
    nested_simple_key = r'{"a":1,"nested":{"a":2,"b":3}}'
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$', nested_simple_key))
    res = r.execute_command('JSON.GET', 'doc2', '$')
    r.assertEqual(res, '[' + nested_simple_key + ']')
    # Set multi existing values
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$..a', '4.2'))
    res = r.execute_command('JSON.GET', 'doc2', '$')
    r.assertEqual(res, '[{"a":4.2,"nested":{"a":4.2,"b":3}}]')


    # Test multi paths
    res = r.execute_command('JSON.GET', 'doc1', '$..tm', '$..nu')
    r.assertEqual(res, '[[[46,876.85],[134.761,"jcoels",null]],[[377,"qda",true]]]')
    # Test multi paths - if one path is none-legacy - result format is not legacy
    res = r.execute_command('JSON.GET', 'doc1', '..tm', '$..nu')
    r.assertEqual(res, '[[[46,876.85],[134.761,"jcoels",null]],[[377,"qda",true]]]')
    # Test missing key
    r.assertIsNone(r.execute_command('JSON.GET', 'docX', '..tm', '$..nu'))
    # Test missing path
    res = r.execute_command('JSON.GET', 'doc1', '..tm', '$..back_in_nov')
    r.assertEqual(res, '[[[46,876.85],[134.761,"jcoels",null]],[]]')
    res = r.execute_command('JSON.GET', 'doc2', '..a', '..b', '$.back_in_nov')
    r.assertEqual(res, '[[4.2,4.2],[3],[]]')

    # Test legacy multi path (all paths are legacy)
    res = r.execute_command('JSON.GET', 'doc1', '..nu', '..tm')
    r.assertEqual(json.loads(res), json.loads('{"..nu":[377,"qda",true],"..tm":[46,876.85]}'))
    # Test legacy single path
    res = r.execute_command('JSON.GET', 'doc1', '..tm')
    r.assertEqual(res, '[46,876.85]')

    # Test missing legacy path (should return an error for a missing path)
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$.nested.b', 'null'))
    r.expect('JSON.GET', 'doc2', '.a', '.nested.b', '.back_in_nov', '.ttyl').raiseError()
    r.expect('JSON.GET', 'doc2', '.back_in_nov').raiseError()


def testMGetCommand(env):
    """Test REJSON.MGET command"""
    r = env
    # Test mget with multi paths
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":1, "b": 2, "nested": {"a": 3}, "c": null, "nested2": {"a": null}} '))
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$', '{"a":4, "b": 5, "nested": {"a": 6}, "c": null, "nested2": {"a": [null]}}'))
    # Compare also to single JSON.GET
    res1 = r.execute_command('JSON.GET', 'doc1', '$..a')
    res2 = r.execute_command('JSON.GET', 'doc2', '$..a')
    r.assertEqual(res1, '[1,3,null]')
    r.assertEqual(res2, '[4,6,[null]]')

    # Test mget with single path
    res = r.execute_command('JSON.MGET', 'doc1', '$..a')
    r.assertEqual([res1], res)
    # Test mget with multi path
    res = r.execute_command('JSON.MGET', 'doc1', 'doc2', '$..a')
    r.assertEqual(res, [res1,res2])

    # Test missing key
    res = r.execute_command('JSON.MGET', 'doc1', 'missing_doc', '$..a')
    r.assertEqual(res, [res1, None])
    res = r.execute_command('JSON.MGET', 'missing_doc1', 'missing_doc2', '$..a')
    r.assertEqual(res, [None, None])


def testNumByCommands(env):
    """
    Test REJSON.NUMINCRBY command
    Test REJSON.NUMMULTBY command
    Test REJSON.NUMPOWBY command
    """
    r = env

    # Test NUMINCRBY
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":"b","b":[{"a":2}, {"a":5.0}, {"a":"c"}]}'))
    # Test multi
    res = r.execute_command('JSON.NUMINCRBY', 'doc1', '$..a', '2')
    r.assertEqual(json.loads(res), [None, 4, 7.0, None])
    res = r.execute_command('JSON.NUMINCRBY', 'doc1', '$..a', '2.5')
    r.assertEqual(json.loads(res), [None, 6.5, 9.5, None])
    # Test single
    res = r.execute_command('JSON.NUMINCRBY', 'doc1', '$.b[1].a', '2')
    #  Avoid json.loads to verify the underlying type (integer/float)
    r.assertEqual(res, '[11.5]')
    res = r.execute_command('JSON.NUMINCRBY', 'doc1', '$.b[2].a', '2')
    r.assertEqual(res, '[null]')
    res = r.execute_command('JSON.NUMINCRBY', 'doc1', '$.b[1].a', '3.5')
    r.assertEqual(res, '[15.0]')

    # Test NUMMULTBY
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":"b","b":[{"a":2}, {"a":5.0}, {"a":"c"}]}'))
    # Test multi
    res = r.execute_command('JSON.NUMMULTBY', 'doc1', '$..a', '2')
    r.assertEqual(json.loads(res), [None, 4, 10, None])
    res = r.execute_command('JSON.NUMMULTBY', 'doc1', '$..a', '2.5')
    #  Avoid json.loads to verify the underlying type (integer/float)
    r.assertEqual(res, '[null,10.0,25.0,null]')
    # Test single
    res = r.execute_command('JSON.NUMMULTBY', 'doc1', '$.b[1].a', '2')
    r.assertEqual(res, '[50.0]')
    res = r.execute_command('JSON.NUMMULTBY', 'doc1', '$.b[2].a', '2')
    r.assertEqual(res, '[null]')
    res = r.execute_command('JSON.NUMMULTBY', 'doc1', '$.b[1].a', '3')
    r.assertEqual(res, '[150.0]')

    # Test NUMPOWBY
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":"b","b":[{"a":2}, {"a":5.0}, {"a":"c"}]}'))
    # Test multi
    res = r.execute_command('JSON.NUMPOWBY', 'doc1', '$..a', '2')
    r.assertEqual(json.loads(res), [None, 4, 25, None])
    #  Avoid json.loads to verify the underlying type (integer/float)
    res = r.execute_command('JSON.NUMPOWBY', 'doc1', '$..a', '2')
    r.assertEqual(res, '[null,16,625.0,null]')
    # Test single
    res = r.execute_command('JSON.NUMPOWBY', 'doc1', '$.b[1].a', '2')
    r.assertEqual(res, '[390625.0]')
    res = r.execute_command('JSON.NUMPOWBY', 'doc1', '$.b[2].a', '2')
    r.assertEqual(res, '[null]')
    res = r.execute_command('JSON.NUMPOWBY', 'doc1', '$.b[1].a', '3')
    r.assertEqual(res, '[5.960464477539062e16]')

    # Test missing key
    r.expect('JSON.NUMINCRBY', 'non_existing_doc', '$..a', '2').raiseError()
    r.expect('JSON.NUMMULTBY', 'non_existing_doc', '$..a', '2').raiseError()
    r.expect('JSON.NUMPOWBY', 'non_existing_doc', '$..a', '2').raiseError()

    # FIXME: Test legacy


def testStrAppendCommand(env):
    """
    Test REJSON.STRAPPEND command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":"foo", "nested1": {"a": "hello"}, "nested2": {"a": 31}}'))
    # Test multi
    res = r.execute_command('JSON.STRAPPEND', 'doc1', '$..a', '"bar"')
    r.assertEqual(res, [6, 8, None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(res, '[{"a":"foobar","nested1":{"a":"hellobar"},"nested2":{"a":31}}]')
    # Test single
    res = r.execute_command('JSON.STRAPPEND', 'doc1', '$.nested1.a', '"baz"')
    r.assertEqual(res, [11])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(res, '[{"a":"foobar","nested1":{"a":"hellobarbaz"},"nested2":{"a":31}}]')

    # Test missing key
    r.expect('JSON.STRAPPEND', 'non_existing_doc', '$..a', '"err"').raiseError()

    # FIXME: Test legacy

def testStrLenCommand(env):
    """
    Test REJSON.STRLEN command
    """
    r = env

    # Test multi
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":"foo", "nested1": {"a": "hello"}, "nested2": {"a": 31}}'))
    res1 = r.execute_command('JSON.STRLEN', 'doc1', '$..a')
    r.assertEqual(res1, [3, 5, None])
    res2 = r.execute_command('JSON.STRAPPEND', 'doc1', '$..a', '"bar"')
    r.assertEqual(res2, [6, 8, None])
    res1 = r.execute_command('JSON.STRLEN', 'doc1', '$..a')
    r.assertEqual(res1, res2)

    # Test single
    res = r.execute_command('JSON.STRLEN', 'doc1', '$.nested1.a')
    r.assertEqual(res, [8])
    res = r.execute_command('JSON.STRLEN', 'doc1', '$.nested2.a')
    r.assertEqual(res, [None])

    # Test missing key
    r.expect('JSON.STRLEN', 'non_existing_doc', '$..a').raiseError()

    # FIXME: Test legacy

def testArrAppendCommand(env):
    """
    Test REJSON.ARRAPPEND command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi
    res = r.execute_command('JSON.ARRAPPEND', 'doc1', '$..a', '"bar"', '"racuda"')
    r.assertEqual(res, [3, 5, None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", None, "world", "bar", "racuda"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRAPPEND', 'doc1', '$.nested1.a', '"baz"')
    r.assertEqual(res, [6])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", None, "world", "bar", "racuda", "baz"]}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRAPPEND', 'non_existing_doc', '$..a').raiseError()

    # Test legacy
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi (all paths are updated, but return result of last path)
    res = r.execute_command('JSON.ARRAPPEND', 'doc1', '..a', '"bar"', '"racuda"')
    r.assertEqual(res, 5)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", None, "world", "bar", "racuda"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRAPPEND', 'doc1', '.nested1.a', '"baz"')
    r.assertEqual(res, 6)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", None, "world", "bar", "racuda", "baz"]}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRAPPEND', 'non_existing_doc', '..a').raiseError()


def testArrInsertCommand(env):
    """
    Test REJSON.ARRINSERT command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi
    res = r.execute_command('JSON.ARRINSERT', 'doc1', '$..a', '1', '"bar"', '"racuda"')
    r.assertEqual(res, [3, 5, None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", "bar", "racuda", None, "world"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRINSERT', 'doc1', '$.nested1.a', -2, '"baz"')
    r.assertEqual(res, [6])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", "bar", "racuda", "baz", None, "world"]}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRINSERT', 'non_existing_doc', '$..a', '0').raiseError()

    # Test legacy
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi (all paths are updated, but return result of last path)
    res = r.execute_command('JSON.ARRINSERT', 'doc1', '..a', '1', '"bar"', '"racuda"')
    r.assertEqual(res, 5)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", "bar", "racuda", None, "world"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRINSERT', 'doc1', '.nested1.a', -2, '"baz"')
    r.assertEqual(res, 6)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo", "bar", "racuda"], "nested1": {"a": ["hello", "bar", "racuda", "baz", None, "world"]}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRINSERT', 'non_existing_doc', '..a').raiseError()


def testArrLenCommand(env):
    """
    Test REJSON.ARRLEN command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi
    res = r.execute_command('JSON.ARRLEN', 'doc1', '$..a')
    r.assertEqual(res, [1, 3, None])
    res = r.execute_command('JSON.ARRAPPEND', 'doc1', '$..a', '"non"', '"abba"', '"stanza"')
    r.assertEqual(res, [4, 6, None])
    r.execute_command('JSON.CLEAR', 'doc1', '$.a')
    res = r.execute_command('JSON.ARRLEN', 'doc1', '$..a')
    r.assertEqual(res, [0, 6, None])
    # Test single
    res = r.execute_command('JSON.ARRLEN', 'doc1', '$.nested1.a')
    r.assertEqual(res, [6])

    # Test missing key
    r.expect('JSON.ARRLEN', 'non_existing_doc', '$..a').raiseError()

    # Test legacy
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi (return result of last path)
    res = r.execute_command('JSON.ARRLEN', 'doc1', '$..a')
    r.assertEqual(res, [1, 3, None])
    res = r.execute_command('JSON.ARRAPPEND', 'doc1', '..a', '"non"', '"abba"', '"stanza"')
    r.assertEqual(res, 6)
    # Test single
    res = r.execute_command('JSON.ARRLEN', 'doc1', '.nested1.a')
    r.assertEqual(res, 6)

    # Test missing key
    r.assertEqual(r.execute_command('JSON.ARRLEN', 'non_existing_doc', '..a'), None)

def testArrPopCommand(env):
    """
    Test REJSON.ARRPOP command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi
    res = r.execute_command('JSON.ARRPOP', 'doc1', '$..a', '1')
    r.assertEqual(res, ['"foo"', 'null', None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": ["hello", "world"]}, "nested2": {"a": 31}}])

    res = r.execute_command('JSON.ARRPOP', 'doc1', '$..a', '-1')
    r.assertEqual(res, [None, '"world"', None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": ["hello"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRPOP', 'doc1', '$.nested1.a', -2)
    r.assertEqual(res, ['"hello"'])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": []}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRPOP', 'non_existing_doc', '$..a', '0').raiseError()

    # Test legacy
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi (all paths are updated, but return result of last path)
    res = r.execute_command('JSON.ARRPOP', 'doc1', '..a', '1')
    r.assertEqual(res, 'null')
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": ["hello", "world"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRPOP', 'doc1', '.nested1.a', -2, '"baz"')
    r.assertEqual(res, '"hello"')
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": ["world"]}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRPOP', 'non_existing_doc', '..a').raiseError()

def testArrTrimCommand(env):
    """
    Test REJSON.ARRTRIM command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi
    res = r.execute_command('JSON.ARRTRIM', 'doc1', '$..a', '1', -1)
    r.assertEqual(res, [0, 2, None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": [None, "world"]}, "nested2": {"a": 31}}])

    res = r.execute_command('JSON.ARRTRIM', 'doc1', '$..a', '1', '1')
    r.assertEqual(res, [0, 1, None])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": ["world"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRTRIM', 'doc1', '$.nested1.a', 1, 0)
    r.assertEqual(res, [0])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": []}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRTRIM', 'non_existing_doc', '$..a', '0').raiseError()

    # Test legacy
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": ["hello", null, "world"]}, "nested2": {"a": 31}}'))
    # Test multi (all paths are updated, but return result of last path)
    res = r.execute_command('JSON.ARRTRIM', 'doc1', '..a', '1', '-1')
    r.assertEqual(res, 2)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": [None, "world"]}, "nested2": {"a": 31}}])
    # Test single
    res = r.execute_command('JSON.ARRTRIM', 'doc1', '.nested1.a', '1', '1')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": [], "nested1": {"a": ["world"]}, "nested2": {"a": 31}}])

    # Test missing key
    r.expect('JSON.ARRTRIM', 'non_existing_doc', '..a').raiseError()

def testObjKeysCommand(env):
    """Test JSON.OBJKEYS command"""
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"nested1": {"a": {"foo": 10, "bar": 20}}, "a":["foo"], "nested2": {"a": {"baz":50}}}'))
    # Test multi
    res = r.execute_command('JSON.OBJKEYS', 'doc1', '$..a')
    r.assertEqual(res, [["foo", "bar"], None, ["baz"]])
    # Test single
    res = r.execute_command('JSON.OBJKEYS', 'doc1', '$.nested1.a')
    r.assertEqual(res, [["foo", "bar"]])

    # Test legacy
    res = r.execute_command('JSON.OBJKEYS', 'doc1', '.*.a')
    r.assertEqual(res, ["foo", "bar"])
    # Test single
    res = r.execute_command('JSON.OBJKEYS', 'doc1', '.nested2.a')
    r.assertEqual(res, ["baz"])

    # Test missing key
    res = r.execute_command('JSON.OBJKEYS', 'non_existing_doc', '..a')
    r.assertEqual(res, None)

    # Test missing key
    r.expect('JSON.OBJKEYS', 'doc1', '$.nowhere').raiseError()


def testObjLenCommand(env):
    """Test JSON.OBJLEN command"""
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"nested1": {"a": {"foo": 10, "bar": 20}}, "a":["foo"], "nested2": {"a": {"baz":50}}}'))
    # Test multi
    res = r.execute_command('JSON.OBJLEN', 'doc1', '$..a')
    r.assertEqual(res, [2, None, 1])
    # Test single
    res = r.execute_command('JSON.OBJLEN', 'doc1', '$.nested1.a')
    r.assertEqual(res, [2])

    # Test missing key
    res = r.execute_command('JSON.OBJLEN', 'non_existing_doc', '$..a')
    r.assertEqual(res, None)

    # Test missing path
    r.expect('JSON.OBJLEN', 'doc1', '$.nowhere').raiseError()


    # Test legacy
    res = r.execute_command('JSON.OBJLEN', 'doc1', '.*.a')
    r.assertEqual(res, 2)
    # Test single
    res = r.execute_command('JSON.OBJLEN', 'doc1', '.nested2.a')
    r.assertEqual(res, 1)

    # Test missing key
    res = r.execute_command('JSON.OBJLEN', 'non_existing_doc', '..a')
    r.assertEqual(res, None)

    # Test missing path
    r.expect('JSON.OBJLEN', 'doc1', '.nowhere').raiseError()


def testTypeCommand(env):
    """Test JSON.TYPE command"""
    types = {
        'null':     None,
        'boolean':  False,
        'integer':  42,
        'number':   1.2,
        'string':   'str',
        'object':   {},
        'array':    [],
    }
    jdata = {}
    jexpected = []
    for i, (k, v) in zip(range(1, len(types)), iter(types.items())):
        jdata["nested" + str(i)] = {'a': v}
        jexpected.append(k)

    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', json.dumps(jdata)))
    # Test multi
    res = r.execute_command('JSON.TYPE', 'doc1', '$..a')
    r.assertEqual(res, jexpected)
    # Test single
    res = r.execute_command('JSON.TYPE', 'doc1', '$.nested2.a')
    r.assertEqual(res, [jexpected[1]])

    # Test legacy
    res = r.execute_command('JSON.TYPE', 'doc1', '..a')
    r.assertEqual(res, jexpected[0])
    # Test missing path (defaults to root)
    res = r.execute_command('JSON.TYPE', 'doc1')
    r.assertEqual(res, 'object')

    # Test missing key
    res = r.execute_command('JSON.TYPE', 'non_existing_doc', '..a')
    r.assertEqual(res, None)

def testClearCommand(env):
    """Test JSON.CLEAR command"""
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"nested1": {"a": {"foo": 10, "bar": 20}}, "a":["foo"], "nested2": {"a": "claro"}, "nested3": {"a": {"baz":50}}}'))
    # Test multi
    res = r.execute_command('JSON.CLEAR', 'doc1', '$..a')
    r.assertEqual(res, 3)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"nested1": {"a": {}}, "a": [], "nested2": {"a": "claro"}, "nested3": {"a": {}}}])

    # Test single
    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"nested1": {"a": {"foo": 10, "bar": 20}}, "a":["foo"], "nested2": {"a": "claro"}, "nested3": {"a": {"baz":50}}}'))
    res = r.execute_command('JSON.CLEAR', 'doc1', '$.nested1.a')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"nested1": {"a": {}}, "a": ["foo"], "nested2": {"a": "claro"}, "nested3": {"a": {"baz": 50}}}])

    # Test missing path (defaults to root)
    res = r.execute_command('JSON.CLEAR', 'doc1')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{}])

    # Test missing key
    r.expect('JSON.CLEAR', 'non_existing_doc', '$..a').raiseError()



def testToggleCommand(env):
    """
    Test REJSON.TOGGLE command
    """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": false}, "nested2": {"a": 31}, "nested3": {"a": true}}'))
    # Test multi
    res = r.execute_command('JSON.TOGGLE', 'doc1', '$..a')
    r.assertEqual(res, [None, 1, None, 0])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo"], "nested1": {"a": True}, "nested2": {"a": 31}, "nested3": {"a": False}}])

    # Test single
    res = r.execute_command('JSON.TOGGLE', 'doc1', '$.nested1.a')
    r.assertEqual(res, [0])
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(json.loads(res), [{"a": ["foo"], "nested1": {"a": False}, "nested2": {"a": 31}, "nested3": {"a": False}}])

    # Test missing key
    r.expect('JSON.TOGGLE', 'non_existing_doc', '$..a').raiseError()

def testDebugCommand(env):
    """
        Test REJSON.DEBUG MEMORY command
            """
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a":["foo"], "nested1": {"a": false}, "nested2": {"a": 31}, "nested3": {"a": true}}'))
    # Test multi
    res = r.execute_command('JSON.DEBUG', 'MEMORY', 'doc1', '$..a')
    r.assertEqual(res, [24, 1, 16, 1])
    # Test single
    res = r.execute_command('JSON.DEBUG', 'MEMORY', 'doc1', '$.nested2.a')
    r.assertEqual(res, [16])

    # Test legacy
    res = r.execute_command('JSON.DEBUG', 'MEMORY', 'doc1', '..a')
    r.assertEqual(res, 24)

    # Test missing key
    r.expect('JSON.DEBUG', 'non_existing_doc', '$..a').raiseError()